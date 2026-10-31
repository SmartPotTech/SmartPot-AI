"""Servidor gRPC de alto rendimiento para SmartPot-AI implementando el contrato ai.proto.

Soporta transporte dual simultáneo:
- Unix Domain Socket (UDS) en /var/run/smartpot/ai.sock para comunicación IPC sub-milisegundo con SmartPot-API-Go.
- TCP en 0.0.0.0:50051 para observabilidad, pruebas y microservicios externos.
"""

import logging
import os
import time
from concurrent import futures
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import grpc

from app.engine.model_verifier import verify_model_manifest
from app.proto.smartpot.v1 import ai_pb2, ai_pb2_grpc
from app.schemas.fleet import FleetCrop
from app.schemas.fleet import FleetRequest as PyFleetRequest
from app.schemas.insight import InsightRequest as PyInsightRequest
from app.schemas.insight import Measures as PyMeasures
from app.schemas.insight import Outside, Place
from app.services import fleet
from app.services.insights import analyze

log = logging.getLogger(__name__)


class SmartPotAIServicer(ai_pb2_grpc.SmartPotAIServiceServicer):
    """Implementación de los procedimientos gRPC definidos en ai.proto."""

    def __init__(self, models: Any, learning: Any = None):
        self.models = models
        self.learning = learning

    def EvaluateInsight(self, request: ai_pb2.InsightRequest, context: grpc.ServicerContext) -> ai_pb2.InsightResponse:
        t0 = time.perf_counter_ns()

        # Conversión de mensaje Protobuf a esquema tipado
        measures_dict = {
            "temperature": request.measures.temperature if request.measures.temperature != 0.0 else None,
            "humidity": request.measures.humidity if request.measures.humidity != 0.0 else None,
            "ph": request.measures.ph if request.measures.ph != 0.0 else None,
            "tds": request.measures.tds if request.measures.tds != 0.0 else None,
            "soil_moisture": request.measures.soil_moisture if request.measures.soil_moisture != 0.0 else None,
            "brightness": request.measures.brightness if request.measures.brightness != 0.0 else None,
            "atmosphere": request.measures.atmosphere if request.measures.atmosphere != 0.0 else None,
        }
        py_measures = PyMeasures(**measures_dict)

        history_list = []
        for h in request.history:
            hm = PyMeasures(
                temperature=h.measures.temperature if h.measures.temperature != 0.0 else None,
                humidity=h.measures.humidity if h.measures.humidity != 0.0 else None,
                ph=h.measures.ph if h.measures.ph != 0.0 else None,
                tds=h.measures.tds if h.measures.tds != 0.0 else None,
                soil_moisture=h.measures.soil_moisture if h.measures.soil_moisture != 0.0 else None,
                brightness=h.measures.brightness if h.measures.brightness != 0.0 else None,
                atmosphere=h.measures.atmosphere if h.measures.atmosphere != 0.0 else None,
                measured_at=datetime.fromtimestamp(h.measured_at_unix_ms / 1000.0, tz=UTC),
            )
            history_list.append(hm)

        place = None
        if request.placement.setting or request.placement.exposure:
            place = Place(
                setting=request.placement.setting or None,
                exposure=request.placement.exposure or None,
            )

        weather = None
        if request.weather.temperature != 0.0 or request.weather.humidity != 0.0:
            weather = Outside(
                temperature=request.weather.temperature,
                humidity=request.weather.humidity,
                precipitation=request.weather.precipitation,
                radiation=request.weather.radiation,
                is_day=request.weather.is_day,
            )

        py_req = PyInsightRequest(
            crop_type=request.crop_type,
            measures=py_measures,
            history=history_list,
            actuators=list(request.actuators),
            local_hour=request.local_hour if request.local_hour >= 0 else None,
            placement=place,
            weather=weather,
        )

        res = analyze(py_req, self.models, self.learning)

        # Mapeo a respuesta Protobuf
        health_by_param = {k: int(v) for k, v in res.health.by_parameter.items()}
        health_proto = ai_pb2.HealthDetail(
            index=int(round(res.health.index)),
            level=res.health.level,
            label=res.health.label,
            by_parameter=health_by_param,
        )

        forecasts_proto = []
        for fc in res.forecasts:
            forecasts_proto.append(
                ai_pb2.ForecastItem(
                    parameter=fc.parameter,
                    trend=fc.trend,
                    slope_per_hour=fc.slope_per_hour,
                    hours_to_limit=fc.hours_to_limit if fc.hours_to_limit is not None else 0.0,
                    limit=fc.limit or "",
                    message=fc.message,
                )
            )

        actions_proto = []
        for a in res.actions:
            actions_proto.append(
                ai_pb2.SuggestedAction(
                    actuator=a.actuator,
                    action=a.action,
                    duration_seconds=a.duration_seconds if a.duration_seconds is not None else 0,
                    reason=a.reason,
                )
            )

        predictions_map = {p.name: float(p.probability) for p in res.predictions}
        conclusions_list = [c.message for c in res.conclusions]
        diagnosis_str = "; ".join(f"{d.parameter}: {d.status}" for d in res.diagnosis)
        placement_advice_str = res.placement.message if res.placement else ""

        eval_nanos = time.perf_counter_ns() - t0

        return ai_pb2.InsightResponse(
            health=health_proto,
            forecasts=forecasts_proto,
            diagnosis=diagnosis_str,
            conclusions=conclusions_list,
            predictions=predictions_map,
            placement_advice=placement_advice_str,
            actions=actions_proto,
            summary=res.summary,
            evaluation_latency_nanos=eval_nanos,
        )

    def AnalyzeFleet(self, request: ai_pb2.FleetRequest, context: grpc.ServicerContext) -> ai_pb2.FleetResponse:
        py_crops = []
        for c in request.crops:
            py_crops.append(
                FleetCrop(
                    id=c.crop_id,
                    name=c.crop_id,
                    crop_type=c.crop_type,
                    measures=PyMeasures(
                        temperature=c.measures.temperature if c.measures.temperature != 0.0 else None,
                        humidity=c.measures.humidity if c.measures.humidity != 0.0 else None,
                        ph=c.measures.ph if c.measures.ph != 0.0 else None,
                        tds=c.measures.tds if c.measures.tds != 0.0 else None,
                        soil_moisture=c.measures.soil_moisture if c.measures.soil_moisture != 0.0 else None,
                        brightness=c.measures.brightness if c.measures.brightness != 0.0 else None,
                        atmosphere=c.measures.atmosphere if c.measures.atmosphere != 0.0 else None,
                    ),
                )
            )

        fleet_res = fleet.analyze(PyFleetRequest(crops=py_crops))

        labels_map = {}
        for idx, group in enumerate(fleet_res.groups):
            for cid in group.crop_ids:
                labels_map[cid] = idx

        shared_issues_msgs = [s.message for s in fleet_res.shared_issues]

        return ai_pb2.FleetResponse(
            cluster_count=len(fleet_res.groups),
            shared_issues=shared_issues_msgs,
            cluster_labels=labels_map,
            fleet_summary=f"Analizados {len(py_crops)} cultivos en {len(fleet_res.groups)} grupos",
        )

    def HotReloadModel(self, request: ai_pb2.ReloadRequest, context: grpc.ServicerContext) -> ai_pb2.ReloadResponse:
        t0 = time.perf_counter_ns()
        crop_type = request.crop_type or None

        # Si se especificó una ruta a modelo/manifiesto, verificar firma
        if request.model_path:
            m_path = Path(request.model_path)
            valid, reason, _ = verify_model_manifest(m_path)
            if not valid:
                return ai_pb2.ReloadResponse(
                    success=False,
                    message=f"Rechazo de seguridad: {reason}",
                    reload_duration_nanos=time.perf_counter_ns() - t0,
                )

        if self.learning and hasattr(self.learning, "reload"):
            self.learning.reload(crop_type)

        duration = time.perf_counter_ns() - t0
        return ai_pb2.ReloadResponse(
            success=True,
            message=f"Modelos recargados en caliente con éxito para {crop_type or 'all'}",
            reload_duration_nanos=duration,
        )


class GrpcServerManager:
    """Administrador del ciclo de vida del servidor gRPC."""

    def __init__(
        self,
        models: Any,
        learning: Any = None,
        port: int = 50051,
        uds_path: str = "/var/run/smartpot/ai.sock",
    ):
        self.models = models
        self.learning = learning
        self.port = port
        self.uds_path = uds_path
        self._server: grpc.Server | None = None

    def start(self) -> None:
        self._server = grpc.server(futures.ThreadPoolExecutor(max_workers=4))
        ai_pb2_grpc.add_SmartPotAIServiceServicer_to_server(
            SmartPotAIServicer(self.models, self.learning),
            self._server,
        )

        # Enlace TCP
        tcp_address = f"0.0.0.0:{self.port}"
        self._server.add_insecure_port(tcp_address)
        log.info("SmartPot-AI: gRPC escuchando en TCP %s", tcp_address)

        # Enlace UDS si la carpeta está disponible
        uds = Path(self.uds_path)
        if uds.parent.exists() and os.access(uds.parent, os.W_OK):
            uds.unlink(missing_ok=True)
            try:
                self._server.add_insecure_port(f"unix://{uds.resolve()}")
                log.info("SmartPot-AI: gRPC escuchando en UDS unix://%s", uds)
            except Exception as error:  # noqa: BLE001
                log.warning("No se pudo enlazar socket Unix en %s (%s); continuando en TCP", uds, error)

        self._server.start()

    def stop(self) -> None:
        if self._server:
            self._server.stop(grace=2.0)
            log.info("SmartPot-AI: servidor gRPC detenido limpiamente")

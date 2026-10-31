"""Servidor gRPC de SmartPot-AI: el transporte rápido entre la API y la IA.

Escucha por socket UNIX (misma máquina, sin pila TCP) y, solo si se pide, por TCP interno. El cuerpo de cada
llamada es el mismo JSON del contrato REST (/v1/insights y /v1/fleet) y se valida con los mismos esquemas, así que
las dos vías responden exactamente lo mismo. Exige el mismo token Bearer que REST en el metadato `authorization`.
"""

import hmac
import logging
import os
from collections.abc import Callable
from concurrent import futures
from pathlib import Path
from typing import Any

import grpc
from pydantic import BaseModel, ValidationError

from app.proto.smartpot.v1 import ai_pb2, ai_pb2_grpc
from app.schemas.fleet import FleetRequest
from app.schemas.insight import InsightRequest
from app.services import fleet
from app.services.insights import analyze

log = logging.getLogger(__name__)


def _dump(model: BaseModel) -> ai_pb2.JsonBody:
    # Igual que FastAPI con response_model: nombres en camelCase (alias).
    return ai_pb2.JsonBody(json=model.model_dump_json(by_alias=True).encode("utf-8"))


class SmartPotAIServicer(ai_pb2_grpc.SmartPotAIServiceServicer):
    """Atiende las llamadas con la misma lógica que las rutas REST."""

    def __init__(self, models: Any, learning: Any = None):
        self.models = models
        self.learning = learning

    def EvaluateInsight(self, request: ai_pb2.JsonBody, context: grpc.ServicerContext) -> ai_pb2.JsonBody:
        try:
            payload = InsightRequest.model_validate_json(request.json)
        except ValidationError as error:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, f"Solicitud inválida: {error.error_count()} errores")
        return _dump(analyze(payload, self.models, self.learning))

    def AnalyzeFleet(self, request: ai_pb2.JsonBody, context: grpc.ServicerContext) -> ai_pb2.JsonBody:
        try:
            payload = FleetRequest.model_validate_json(request.json)
        except ValidationError as error:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, f"Solicitud inválida: {error.error_count()} errores")
        return _dump(fleet.analyze(payload))


class TokenInterceptor(grpc.ServerInterceptor):
    """Rechaza toda llamada sin el token de servicio (comparación en tiempo constante)."""

    def __init__(self, token: str):
        self._expected = f"Bearer {token}".encode()

        def deny(_request: Any, context: grpc.ServicerContext) -> None:
            context.abort(grpc.StatusCode.UNAUTHENTICATED, "Token de servicio inválido")

        self._deny = grpc.unary_unary_rpc_method_handler(deny)

    def intercept_service(self, continuation: Callable, handler_call_details: grpc.HandlerCallDetails) -> Any:
        presented = dict(handler_call_details.invocation_metadata or ()).get("authorization", "")
        if hmac.compare_digest(presented.encode(), self._expected):
            return continuation(handler_call_details)
        return self._deny


class GrpcServerManager:
    """Ciclo de vida del servidor gRPC: socket UNIX si su carpeta es escribible y TCP solo si tcp_port > 0."""

    def __init__(self, models: Any, token: str, learning: Any = None, uds_path: str = "", tcp_port: int = 0):
        self.models = models
        self.learning = learning
        self.token = token
        self.uds_path = uds_path
        self.tcp_port = tcp_port
        self._server: grpc.Server | None = None

    def start(self) -> bool:
        server = grpc.server(futures.ThreadPoolExecutor(max_workers=8), interceptors=[TokenInterceptor(self.token)])
        ai_pb2_grpc.add_SmartPotAIServiceServicer_to_server(SmartPotAIServicer(self.models, self.learning), server)
        bound = []
        if self.uds_path:
            uds = Path(self.uds_path)
            if uds.parent.is_dir() and os.access(uds.parent, os.W_OK):
                uds.unlink(missing_ok=True)
                server.add_insecure_port(f"unix://{uds.resolve()}")
                bound.append(f"unix://{uds}")
            else:
                log.warning("gRPC: la carpeta de %s no es escribible; no se abre el socket UNIX", uds)
        if self.tcp_port:
            server.add_insecure_port(f"0.0.0.0:{self.tcp_port}")
            bound.append(f"tcp:{self.tcp_port}")
        if not bound:
            log.info("gRPC apagado: sin socket UNIX ni puerto TCP; la API usará REST")
            return False
        server.start()
        self._server = server
        log.info("gRPC escuchando en %s", ", ".join(bound))
        return True

    def stop(self) -> None:
        if self._server:
            self._server.stop(grace=2.0)
            log.info("gRPC detenido")

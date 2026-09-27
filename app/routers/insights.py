from fastapi import APIRouter, Depends, Request

from app.core.security import require_service_token
from app.knowledge.profiles import PROFILES
from app.schemas.fleet import FleetRequest, FleetResponse
from app.schemas.insight import InsightRequest, InsightResponse
from app.schemas.profiles import CropProfileOut
from app.services import fleet
from app.services.insights import analyze

router = APIRouter(prefix="/v1", tags=["Asistente"], dependencies=[Depends(require_service_token)])


@router.post("/insights", response_model=InsightResponse, summary="Evaluar una lectura",
             description="Diagnóstico por variable, conclusiones del sistema experto, índice difuso de salud, "
                         "predicciones de los modelos y acciones del agente.")
def insights(payload: InsightRequest, request: Request) -> InsightResponse:
    return analyze(payload, request.app.state.models, getattr(request.app.state, "learning", None))


@router.post("/fleet", response_model=FleetResponse, summary="Analizar todos los cultivos",
             description="Ranking por salud, problemas compartidos del entorno, grupos por condiciones similares "
                         "(K-Means) y acciones agregadas por actuador para aplicarlas en bloque.")
def fleet_analysis(payload: FleetRequest) -> FleetResponse:
    return fleet.analyze(payload)


@router.get("/crop-profiles", response_model=list[CropProfileOut], summary="Base de conocimiento por especie")
def crop_profiles() -> list[CropProfileOut]:
    return [CropProfileOut.from_profile(profile) for profile in PROFILES.values()]


@router.get("/models", summary="Exactitud de los modelos entrenados al arrancar")
def models(request: Request) -> dict[str, float]:
    return {name: round(value, 4) for name, value in request.app.state.models.metrics.items()}

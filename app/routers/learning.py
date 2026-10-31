from typing import Annotated, Any

import httpx
from fastapi import APIRouter, Depends, HTTPException, Request
from starlette.concurrency import run_in_threadpool

from app.core.config import Settings, get_settings
from app.core.security import require_service_token
from app.learning.service import LearningService, to_stored
from app.schemas.insight import CamelModel
from app.schemas.learning import IngestResult, LearningBatch, LearningStatus, TrainRequest

router = APIRouter(prefix="/v1/learning", tags=["Aprendizaje"], dependencies=[Depends(require_service_token)])


class ReloadRequest(CamelModel):
    crop_type: str | None = None


def _service(request: Request) -> LearningService:
    return request.app.state.learning


@router.post(
    "/reload",
    summary="Recargar modelos aprendidos en memoria",
    description="Permite a SmartPot-Trainer notificar que se generó un nuevo artefacto de modelo "
    "para cargarlo en memoria caliente (< 5 ms) sin reiniciar el servicio.",
)
def reload_models(payload: ReloadRequest | None = None, request: Request = None) -> dict[str, Any]:
    service = _service(request)
    crop_type = payload.crop_type if payload else None
    reloaded = service.reload(crop_type)
    return {"status": "OK", "reloaded": reloaded}


@router.post(
    "/readings",
    response_model=IngestResult,
    summary="Recibir lecturas reales",
    description="La API envía por lotes las lecturas que llegan de los cultivos reales. Se guardan con el id "
    "del cultivo seudonimizado y alimentan el próximo entrenamiento.",
)
async def ingest(
    batch: LearningBatch,
    request: Request,
    settings: Annotated[Settings, Depends(get_settings)],
) -> IngestResult:
    if settings.smartpot_trainer_url:
        url = f"{settings.smartpot_trainer_url.rstrip('/')}/v1/learning/readings"
        headers = {"Authorization": request.headers.get("Authorization", "")}
        async with httpx.AsyncClient(timeout=10.0) as client:
            res = await client.post(url, json=batch.model_dump(by_alias=True), headers=headers)
            if not res.is_success:
                raise HTTPException(status_code=res.status_code, detail=res.text)
            return IngestResult.model_validate(res.json())

    service = _service(request)
    stored = service.ingest(
        [
            to_stored(
                item.crop_id,
                item.crop_type,
                item.measured_at.timestamp(),
                item.local_hour,
                item.measures.as_dict(),
            )
            for item in batch.readings
        ]
    )
    return IngestResult(received=len(batch.readings), stored=stored, total=service.store.total())


@router.get(
    "/status",
    response_model=LearningStatus,
    summary="Qué ha aprendido el asistente",
    description="Lecturas por especie, calidad de los datos, comparación de modelos, estados de operación "
    "y detector de lecturas atípicas.",
)
async def status(
    request: Request,
    settings: Annotated[Settings, Depends(get_settings)],
) -> LearningStatus:
    if settings.smartpot_trainer_url:
        url = f"{settings.smartpot_trainer_url.rstrip('/')}/v1/learning/status"
        headers = {"Authorization": request.headers.get("Authorization", "")}
        async with httpx.AsyncClient(timeout=10.0) as client:
            res = await client.get(url, headers=headers)
            if not res.is_success:
                raise HTTPException(status_code=res.status_code, detail=res.text)
            return LearningStatus.model_validate(res.json())

    return LearningStatus.model_validate(_service(request).status())


@router.post(
    "/train",
    response_model=LearningStatus,
    summary="Entrenar ahora",
    description="Entrena la especie indicada, o todas las que tienen datos nuevos suficientes.",
)
async def train(
    payload: TrainRequest,
    request: Request,
    settings: Annotated[Settings, Depends(get_settings)],
) -> LearningStatus:
    if settings.smartpot_trainer_url:
        url = f"{settings.smartpot_trainer_url.rstrip('/')}/v1/learning/train"
        headers = {"Authorization": request.headers.get("Authorization", "")}
        async with httpx.AsyncClient(timeout=60.0) as client:
            res = await client.post(url, json=payload.model_dump(by_alias=True), headers=headers)
            if not res.is_success:
                raise HTTPException(status_code=res.status_code, detail=res.text)
            return LearningStatus.model_validate(res.json())

    service = _service(request)
    crop_types = [payload.crop_type.upper()] if payload.crop_type else service.due()
    for crop_type in crop_types:
        if crop_type not in service.counts():
            raise HTTPException(status_code=404, detail=f"No hay lecturas de {crop_type}")
        await run_in_threadpool(service.train, crop_type)
    return LearningStatus.model_validate(service.status())


@router.delete(
    "/crops/{crop_id}",
    summary="Olvidar un cultivo",
    description="Borra las lecturas de un cultivo eliminado; los modelos lo dejan de usar en el próximo "
    "entrenamiento.",
)
async def forget(
    crop_id: str,
    request: Request,
    settings: Annotated[Settings, Depends(get_settings)],
) -> dict[str, int]:
    if settings.smartpot_trainer_url:
        url = f"{settings.smartpot_trainer_url.rstrip('/')}/v1/learning/crops/{crop_id}"
        headers = {"Authorization": request.headers.get("Authorization", "")}
        async with httpx.AsyncClient(timeout=10.0) as client:
            res = await client.delete(url, headers=headers)
            if not res.is_success:
                raise HTTPException(status_code=res.status_code, detail=res.text)
            return res.json()

    return {"deleted": _service(request).forget(crop_id)}

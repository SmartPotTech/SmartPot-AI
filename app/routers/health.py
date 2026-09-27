from fastapi import APIRouter, Request

router = APIRouter(tags=["Salud"])


@router.get("/health", summary="Estado del servicio")
def health(request: Request) -> dict[str, str]:
    ready = getattr(request.app.state, "models", None) is not None
    learning = getattr(request.app.state, "learning", None)
    return {"status": "UP" if ready else "STARTING", "models": "READY" if ready else "TRAINING",
            "learning": "OFF" if learning is None else "PERSISTENT" if learning.store.persistent else "MEMORY"}

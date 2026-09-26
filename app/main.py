import logging
from contextlib import asynccontextmanager

from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse

from app.core.config import get_settings
from app.engine.models import ModelRegistry
from app.routers import health, insights


def create_app() -> FastAPI:
    settings = get_settings()
    logging.basicConfig(level=settings.log_level.upper(), format="%(asctime)s %(levelname)s %(name)s - %(message)s")

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        app.state.models = ModelRegistry(seed=settings.model_seed)
        yield

    app = FastAPI(
        title="SmartPot AI",
        version="1.0.0",
        description="Servicio interno de SmartPot: sistema experto, lógica difusa, modelos de ML y agente reactivo.",
        lifespan=lifespan,
        docs_url="/docs" if settings.docs_enabled else None,
        redoc_url=None,
        openapi_url="/openapi.json" if settings.docs_enabled else None,
    )

    @app.exception_handler(RequestValidationError)
    async def validation_error(request: Request, exc: RequestValidationError) -> JSONResponse:
        errors = [{"field": ".".join(str(p) for p in e["loc"][1:]), "message": e["msg"]} for e in exc.errors()]
        return JSONResponse(status_code=422, content={"detail": "Datos inválidos", "errors": errors})

    app.include_router(health.router)
    app.include_router(insights.router)
    return app


app = create_app()

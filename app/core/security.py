import secrets
from typing import Annotated

from fastapi import Depends, HTTPException, status
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer

from app.core.config import Settings, get_settings

bearer = HTTPBearer(auto_error=False)


def require_service_token(
        credentials: Annotated[HTTPAuthorizationCredentials | None, Depends(bearer)],
        settings: Annotated[Settings, Depends(get_settings)],
) -> None:
    """El servicio es interno: solo la API, con el token compartido, puede consultarlo."""
    if credentials is None or not secrets.compare_digest(credentials.credentials, settings.smartpot_ai_token):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Token de servicio inválido",
            headers={"WWW-Authenticate": "Bearer"},
        )

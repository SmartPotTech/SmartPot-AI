from functools import lru_cache

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", env_file_encoding="utf-8", extra="ignore")

    smartpot_ai_token: str = Field(min_length=24, description="Token que la API envía como Bearer")
    docs_enabled: bool = False
    log_level: str = "INFO"
    model_seed: int = 42


@lru_cache
def get_settings() -> Settings:
    return Settings()

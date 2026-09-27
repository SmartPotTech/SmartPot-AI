from functools import lru_cache

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", env_file_encoding="utf-8", extra="ignore")

    smartpot_ai_token: str = Field(min_length=24, description="Token que la API envía como Bearer")
    docs_enabled: bool = False
    log_level: str = "INFO"
    model_seed: int = 42

    # Aprendizaje con lecturas reales. data_dir vacío: todo en memoria.
    data_dir: str = "data"
    learning_min_samples: int = Field(200, ge=50)
    learning_retrain_every: int = Field(300, ge=10)
    learning_check_seconds: int = Field(300, ge=0, description="0 apaga el reentrenamiento en segundo plano")
    learning_retention_days: int = Field(60, ge=1)
    learning_max_rows: int = Field(200_000, ge=1_000)
    learning_training_rows: int = Field(12_000, ge=500)
    learning_separate_process: bool = True


@lru_cache
def get_settings() -> Settings:
    return Settings()

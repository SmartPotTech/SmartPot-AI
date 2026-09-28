from datetime import datetime
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator
from pydantic.alias_generators import to_camel

from app.knowledge.profiles import PROFILES


class CamelModel(BaseModel):
    model_config = ConfigDict(alias_generator=to_camel, populate_by_name=True)


class Measures(CamelModel):
    temperature: float | None = Field(None, ge=-20, le=60)
    humidity: float | None = Field(None, ge=0, le=100)
    brightness: float | None = Field(None, ge=0, le=200_000)
    ph: float | None = Field(None, ge=0, le=14)
    tds: float | None = Field(None, ge=0, le=10_000)
    atmosphere: float | None = Field(None, ge=300, le=1_100)
    soil_moisture: float | None = Field(None, ge=0, le=100)
    measured_at: datetime | None = None

    def as_dict(self) -> dict[str, float | None]:
        return self.model_dump(by_alias=True, exclude={"measured_at"})


class Place(CamelModel):
    setting: Literal["INDOOR", "OUTDOOR"] | None = None
    exposure: Literal["FULL_SUN", "PARTIAL_SUN", "SHADE"] | None = None


class Outside(CamelModel):
    """Clima actual del lugar del cultivo."""
    temperature: float = Field(ge=-60, le=60)
    humidity: float = Field(ge=0, le=100)
    precipitation: float = Field(0, ge=0, le=500)
    radiation: float = Field(0, ge=0, le=1500)
    is_day: bool = True
    condition: str | None = Field(None, max_length=20)


class InsightRequest(CamelModel):
    crop_type: str
    measures: Measures
    history: list[Measures] = Field(default_factory=list, max_length=500)
    actuators: list[str] = Field(default_factory=list, max_length=20)
    local_hour: int | None = Field(None, ge=0, le=23)
    placement: Place | None = None
    weather: Outside | None = None

    @field_validator("crop_type")
    @classmethod
    def known_crop(cls, value: str) -> str:
        if value.upper() not in PROFILES:
            raise ValueError(f"Tipo de cultivo desconocido: {value}")
        return value.upper()


class Health(CamelModel):
    index: float
    level: str
    label: str
    by_parameter: dict[str, float] = Field(default_factory=dict,
                                           description="Salud de 0 a 100 de cada variable: explica el índice")


class Diagnosis(CamelModel):
    parameter: str
    value: float
    status: str
    severity: str
    message: str
    recommendation: str | None = None


class Conclusion(CamelModel):
    rule: str
    title: str
    message: str
    certainty: float


class Prediction(CamelModel):
    name: str
    label: str
    probability: float
    model: str


class Forecast(CamelModel):
    parameter: str
    current: float
    slope_per_hour: float
    # Alias explícito: el generador camelCase produciría «expectedIn3H».
    expected_in_3h: float = Field(alias="expectedIn3h")
    trend: str
    hours_to_limit: float | None = None
    limit: str | None = None
    confidence: float
    message: str


class Action(CamelModel):
    actuator: str
    action: str
    duration_seconds: int | None = None
    reason: str


class LearnedPrediction(CamelModel):
    name: str
    label: str
    probability: float
    model: str
    metric: str
    score: float | None = None


class LearnedMoisture(CamelModel):
    # Alias explícito por la misma razón que expectedIn3h.
    expected_in_1h: float = Field(alias="expectedIn1h")
    model: str
    mae: float | None = None


class OperatingState(CamelModel):
    label: str
    description: str
    share: float


class LearnedAnomaly(CamelModel):
    score: float
    unusual: bool


class Learning(CamelModel):
    source: str = Field(description="LEARNED si hay modelos entrenados con lecturas reales; BASE si aún no")
    readings: int
    trained_at: datetime | None = None
    message: str
    predictions: list[LearnedPrediction] = Field(default_factory=list)
    moisture: LearnedMoisture | None = None
    state: OperatingState | None = None
    anomaly: LearnedAnomaly | None = None


class PlacementAdvice(CamelModel):
    level: str = Field(description="OK, UNKNOWN (sin lugar), TIP (no es el ideal) o MOVE (ya afecta la salud)")
    title: str
    message: str
    light_need: str = Field(description="Luz que pide la especie: FULL_SUN o PARTIAL_SUN")
    ideal_setting: str
    ideal_exposure: str


class InsightResponse(CamelModel):
    crop_type: str
    health: Health
    diagnosis: list[Diagnosis]
    conclusions: list[Conclusion]
    predictions: list[Prediction]
    actions: list[Action]
    forecasts: list[Forecast] = Field(default_factory=list)
    learning: Learning | None = None
    placement: PlacementAdvice | None = None
    summary: str

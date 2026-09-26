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

    def as_dict(self) -> dict[str, float | None]:
        return self.model_dump(by_alias=True)


class InsightRequest(CamelModel):
    crop_type: str
    measures: Measures
    history: list[Measures] = Field(default_factory=list, max_length=500)
    actuators: list[str] = Field(default_factory=list, max_length=20)
    local_hour: int | None = Field(None, ge=0, le=23)

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


class Action(CamelModel):
    actuator: str
    action: str
    duration_seconds: int | None = None
    reason: str


class InsightResponse(CamelModel):
    crop_type: str
    health: Health
    diagnosis: list[Diagnosis]
    conclusions: list[Conclusion]
    predictions: list[Prediction]
    actions: list[Action]
    summary: str

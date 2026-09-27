from pydantic import Field, field_validator

from app.knowledge.profiles import PROFILES
from app.schemas.insight import CamelModel, Health, Measures


class FleetCrop(CamelModel):
    id: str = Field(min_length=1, max_length=64)
    name: str = Field(min_length=1, max_length=80)
    crop_type: str
    measures: Measures | None = None
    actuators: list[str] = Field(default_factory=list, max_length=20)

    @field_validator("crop_type")
    @classmethod
    def known_crop(cls, value: str) -> str:
        if value.upper() not in PROFILES:
            raise ValueError(f"Tipo de cultivo desconocido: {value}")
        return value.upper()


class FleetRequest(CamelModel):
    crops: list[FleetCrop] = Field(min_length=1, max_length=50)
    local_hour: int | None = Field(None, ge=0, le=23)


class FleetCropResult(CamelModel):
    id: str
    name: str
    crop_type: str
    rank: int | None = Field(None, description="1 = el que más atención necesita")
    health: Health | None = None
    issues: list[str] = Field(default_factory=list, description="Variables fuera de rango, en español")


class SharedIssue(CamelModel):
    parameter: str
    status: str
    crop_ids: list[str]
    share: float
    message: str


class CropGroup(CamelModel):
    label: str
    crop_ids: list[str]
    description: str


class FleetAction(CamelModel):
    actuator: str
    action: str
    duration_seconds: int | None = None
    crop_ids: list[str]
    reason: str


class FleetResponse(CamelModel):
    average_health: float | None
    crops: list[FleetCropResult]
    shared_issues: list[SharedIssue]
    groups: list[CropGroup]
    actions: list[FleetAction]
    summary: str

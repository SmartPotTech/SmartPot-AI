from app.knowledge.profiles import CropProfile
from app.schemas.insight import CamelModel


class RangeOut(CamelModel):
    min: float
    max: float
    unit: str
    label: str


class CropProfileOut(CamelModel):
    type: str
    name: str
    description: str
    ranges: dict[str, RangeOut]

    @classmethod
    def from_profile(cls, profile: CropProfile) -> "CropProfileOut":
        return cls(
            type=profile.type,
            name=profile.name,
            description=profile.description,
            ranges={name: RangeOut(min=r.min, max=r.max, unit=r.unit, label=r.label)
                    for name, r in profile.ranges.items()},
        )

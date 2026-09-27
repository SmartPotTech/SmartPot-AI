"""Convierte la petición de flota en la entrada del motor y su resultado en la respuesta HTTP."""

from app.engine.fleet import CropInput, analyze_fleet, phrase
from app.schemas.fleet import CropGroup, FleetAction, FleetCropResult, FleetRequest, FleetResponse, SharedIssue
from app.schemas.insight import Health


def analyze(request: FleetRequest) -> FleetResponse:
    crops = [CropInput(id=crop.id, name=crop.name, crop_type=crop.crop_type,
                       measures=crop.measures.as_dict() if crop.measures else None,
                       actuators={a.upper() for a in crop.actuators})
             for crop in request.crops]
    result = analyze_fleet(crops, request.local_hour)
    return FleetResponse(
        average_health=result.average_health,
        crops=[FleetCropResult(
            id=c.id, name=c.name, crop_type=c.crop_type, rank=c.rank or None,
            health=Health(index=c.health.index, level=c.health.level, label=c.health.label,
                          by_parameter=c.health.by_parameter) if c.health else None,
            issues=[_capitalize(phrase(d.parameter, d.status)) for d in c.diagnosis if d.status in ("LOW", "HIGH")])
            for c in result.crops],
        shared_issues=[SharedIssue(parameter=s.parameter, status=s.status, crop_ids=s.crop_ids, share=s.share,
                                   message=s.message) for s in result.shared_issues],
        groups=[CropGroup(label=g.label, crop_ids=g.crop_ids, description=g.description) for g in result.groups],
        actions=[FleetAction(actuator=a.actuator, action=a.action, duration_seconds=a.duration_seconds,
                             crop_ids=a.crop_ids, reason=a.reason) for a in result.actions],
        summary=result.summary,
    )


def _capitalize(text: str) -> str:
    return text[:1].upper() + text[1:]

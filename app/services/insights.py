"""Orquesta el análisis: diagnóstico → modelos → sistema experto → lógica difusa → agente."""

from app.engine import agent
from app.engine.diagnosis import diagnose, is_rest_hour
from app.engine.expert_system import InferenceEngine, WorkingMemory
from app.engine.fuzzy import health_index
from app.engine.models import ModelRegistry, normalize
from app.engine.rules import RULES
from app.knowledge.profiles import CropProfile, get_profile
from app.schemas.insight import (
    Action,
    Conclusion,
    Diagnosis,
    Health,
    InsightRequest,
    InsightResponse,
    Prediction,
)

ENGINE = InferenceEngine(RULES)


def analyze(request: InsightRequest, models: ModelRegistry) -> InsightResponse:
    profile = get_profile(request.crop_type)
    measures = request.measures.as_dict()
    diagnosis = diagnose(profile, measures, request.local_hour)

    positions = normalize(profile, measures)
    history = [normalize(profile, item.as_dict()) for item in request.history]
    predictions = {
        "ventilation": models.ventilation_probability(positions),
        "ph_correction": models.ph_correction_probability(positions),
        "anomaly": models.anomaly_probability(positions, history) if positions else None,
    }

    memory = WorkingMemory()
    for item in diagnosis:
        memory.assert_fact(f"status:{item.parameter}", item.status)
        memory.assert_fact(f"severity:{item.parameter}", item.severity)
    for name, probability in predictions.items():
        if probability is not None:
            memory.assert_fact(f"prediction:{name}", probability)
    firings = ENGINE.run(memory)

    health = health_index({item.parameter: item.deviation for item in diagnosis})
    actions = agent.decide(diagnosis, predictions, memory.facts, {a.upper() for a in request.actuators},
                           resting=is_rest_hour(request.local_hour))

    return InsightResponse(
        crop_type=profile.type,
        health=Health(index=health.index, level=health.level, label=health.label),
        diagnosis=[Diagnosis(parameter=d.parameter, value=d.value, status=d.status, severity=d.severity,
                             message=d.message, recommendation=d.recommendation) for d in diagnosis],
        conclusions=[Conclusion(rule=f.rule, title=f.title, message=f.message, certainty=f.certainty)
                     for f in firings],
        predictions=_predictions(predictions),
        actions=[Action(actuator=a.actuator, action=a.action, duration_seconds=a.duration_seconds, reason=a.reason)
                 for a in actions],
        summary=_summary(profile, health.index, health.label, diagnosis, len(actions)),
    )


def _predictions(values: dict[str, float | None]) -> list[Prediction]:
    catalog = {
        "ventilation": ("Necesidad de ventilación", "Regresión logística"),
        "ph_correction": ("Necesidad de corregir el pH", "Red neuronal (MLP)"),
        "anomaly": ("Lectura atípica", "Isolation Forest"),
    }
    return [Prediction(name=name, label=catalog[name][0], probability=round(value, 3), model=catalog[name][1])
            for name, value in values.items() if value is not None]


def _summary(profile: CropProfile, index: float, label: str, diagnosis, action_count: int) -> str:
    issues = sorted((d for d in diagnosis if d.status not in ("OPTIMAL", "REST")), key=lambda d: d.deviation,
                    reverse=True)
    text = f"Tu {profile.name.lower()} está en estado «{label}» ({index:.0f}/100)."
    if not issues:
        return text + " Todas las variables medidas están en su rango ideal."
    names = [profile.ranges[d.parameter].label for d in issues[:2]]
    text += " Revisa " + " y ".join(names) + "."
    if action_count:
        text += f" El asistente propone {action_count} {'acción' if action_count == 1 else 'acciones'}."
    return text

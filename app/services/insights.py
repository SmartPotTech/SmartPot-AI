"""Orquesta el análisis: diagnóstico → pronóstico → modelos base y aprendidos → sistema experto → lógica difusa
→ agente."""

import time
from datetime import UTC, datetime

from app.engine import agent, placement
from app.engine.diagnosis import diagnose, is_rest_hour
from app.engine.expert_system import InferenceEngine, WorkingMemory
from app.engine.forecast import forecast
from app.engine.fuzzy import health_index
from app.engine.models import ModelRegistry, normalize
from app.engine.rules import RULES
from app.knowledge.profiles import CropProfile, get_profile
from app.learning.service import LearningService
from app.schemas.insight import (
    Action,
    Conclusion,
    Diagnosis,
    Forecast,
    Health,
    InsightRequest,
    InsightResponse,
    Learning,
    PlacementAdvice,
    Prediction,
)

ENGINE = InferenceEngine(RULES)


def analyze(request: InsightRequest, models: ModelRegistry, learning: LearningService | None = None) -> InsightResponse:
    profile = get_profile(request.crop_type)
    measures = request.measures.as_dict()
    diagnosis = diagnose(profile, measures, request.local_hour)

    timed = [(_utc(item.measured_at), item.as_dict()) for item in request.history if item.measured_at]
    forecasts = forecast(profile, timed)
    learned = _learned(learning, request, measures, timed)

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
        memory.assert_fact(f"value:{item.parameter}", item.value)
        memory.assert_fact(f"severity:{item.parameter}", item.severity)
    for name, probability in predictions.items():
        if probability is not None:
            memory.assert_fact(f"prediction:{name}", probability)
    for item in forecasts:
        memory.assert_fact(f"forecast:{item.parameter}", {"trend": item.trend, "limit": item.limit,
                                                          "hours": item.hours_to_limit})
    place = request.placement
    if place and place.setting:
        memory.assert_fact("placement:setting", place.setting)
    if place and place.exposure:
        memory.assert_fact("placement:exposure", place.exposure)
    if request.weather:
        memory.assert_fact("outside", request.weather.model_dump())
    if learned:
        for item in learned.predictions:
            memory.assert_fact(f"learned:{item.name}", item.probability)
        if learned.anomaly and learned.anomaly.unusual:
            memory.assert_fact("learned:unusual", learned.anomaly.score)
    firings = ENGINE.run(memory)

    health = health_index({item.parameter: item.deviation for item in diagnosis})
    actions = agent.decide(diagnosis, predictions, memory.facts, {a.upper() for a in request.actuators},
                           resting=is_rest_hour(request.local_hour), forecasts=forecasts)
    sunny = bool(request.weather and request.weather.is_day and request.weather.radiation >= 300)
    advice = placement.advise(profile, place.setting if place else None, place.exposure if place else None,
                              diagnosis, sunny_outside=sunny)

    return InsightResponse(
        crop_type=profile.type,
        health=Health(index=health.index, level=health.level, label=health.label, by_parameter=health.by_parameter),
        diagnosis=[Diagnosis(parameter=d.parameter, value=d.value, status=d.status, severity=d.severity,
                             message=d.message, recommendation=d.recommendation) for d in diagnosis],
        conclusions=[Conclusion(rule=f.rule, title=f.title, message=f.message, certainty=f.certainty)
                     for f in firings],
        predictions=_predictions(predictions),
        actions=[Action(actuator=a.actuator, action=a.action, duration_seconds=a.duration_seconds, reason=a.reason)
                 for a in actions],
        forecasts=[Forecast(parameter=f.parameter, current=f.current, slope_per_hour=f.slope_per_hour,
                            expected_in_3h=f.expected_in_3h, trend=f.trend, hours_to_limit=f.hours_to_limit,
                            limit=f.limit, confidence=f.confidence, message=f.message) for f in forecasts],
        learning=learned,
        placement=PlacementAdvice(level=advice.level, title=advice.title, message=advice.message,
                                  light_need=advice.light_need, ideal_setting=advice.ideal_setting,
                                  ideal_exposure=advice.ideal_exposure),
        summary=_summary(profile, health.index, health.label, diagnosis, len(actions), advice.level == "MOVE"),
    )


def _learned(learning: LearningService | None, request: InsightRequest, measures: dict,
             timed: list) -> Learning | None:
    """Lo aprendido de las lecturas reales de la especie; nunca impide la evaluación."""
    if learning is None:
        return None
    moment = _utc(request.measures.measured_at).timestamp() if request.measures.measured_at else time.time()
    history = [(when.timestamp(), values) for when, values in timed]
    data = learning.infer(request.crop_type, measures, request.local_hour, moment, history)
    trained_at = data.get("trained_at")
    moisture = data.get("moisture")
    return Learning.model_validate({
        **data,
        "trained_at": datetime.fromtimestamp(trained_at, UTC) if trained_at else None,
        "moisture": {"expectedIn1h": moisture["expected"], "model": moisture["model"], "mae": moisture["mae"]}
        if moisture else None,
    })


def _utc(moment):
    return moment if moment.tzinfo else moment.replace(tzinfo=UTC)


def _predictions(values: dict[str, float | None]) -> list[Prediction]:
    catalog = {
        "ventilation": ("Necesidad de ventilación", "Regresión logística"),
        "ph_correction": ("Necesidad de corregir el pH", "Red neuronal (MLP)"),
        "anomaly": ("Lectura atípica", "Isolation Forest"),
    }
    return [Prediction(name=name, label=catalog[name][0], probability=round(value, 3), model=catalog[name][1])
            for name, value in values.items() if value is not None]


def _summary(profile: CropProfile, index: float, label: str, diagnosis, action_count: int,
             move: bool = False) -> str:
    issues = sorted((d for d in diagnosis if d.status not in ("OPTIMAL", "REST")), key=lambda d: d.deviation,
                    reverse=True)
    text = f"Tu {profile.name.lower()} está en estado «{label}» ({index:.0f}/100)."
    if not issues:
        return text + " Todas las variables medidas están en su rango ideal."
    names = [profile.ranges[d.parameter].label for d in issues[:2]]
    text += " Revisa " + " y ".join(names) + "."
    if action_count:
        text += f" El asistente propone {action_count} {'acción' if action_count == 1 else 'acciones'}."
    if move:
        text += " También conviene cambiar el cultivo de lugar."
    return text

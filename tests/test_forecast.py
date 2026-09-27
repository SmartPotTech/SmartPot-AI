from datetime import UTC, datetime, timedelta

import numpy as np

from app.engine.agent import decide
from app.engine.diagnosis import diagnose
from app.engine.forecast import forecast, theil_sen
from app.knowledge.profiles import get_profile

START = datetime(2026, 9, 26, 12, tzinfo=UTC)


def series(**values_per_step: tuple[float, float]) -> list[tuple[datetime, dict]]:
    """12 lecturas cada 10 minutos; cada variable arranca en un valor y cambia un paso fijo."""
    points = []
    for step in range(12):
        measures = {name: start + delta * step for name, (start, delta) in values_per_step.items()}
        points.append((START + timedelta(minutes=10 * step), measures))
    return points


def by_parameter(results):
    return {f.parameter: f for f in results}


def test_theil_sen_ignores_a_single_outlier():
    hours = np.arange(10, dtype=float)
    values = 2.0 * hours + 1
    values[4] = 90
    slope, intercept, agreement = theil_sen(hours, values)
    assert abs(slope - 2.0) < 0.01
    assert abs(intercept - 1.0) < 0.1
    assert agreement > 0.7


def test_drying_substrate_predicts_time_to_minimum():
    # Lechuga: sustrato ideal 60–80 %; baja 1 % cada 10 min (6 % por hora) desde 75 %.
    result = by_parameter(forecast(get_profile("LETTUCE"), series(soilMoisture=(75, -1))))["soilMoisture"]
    assert result.trend == "FALLING"
    assert result.limit == "MIN"
    assert abs(result.slope_per_hour + 6) < 0.01
    assert 0.5 < result.hours_to_limit < 0.8
    assert "llegará al mínimo" in result.message


def test_steady_values_are_stable():
    result = by_parameter(forecast(get_profile("TOMATO"), series(temperature=(24, 0.01))))["temperature"]
    assert result.trend == "STABLE"
    assert result.hours_to_limit is None


def test_short_histories_are_not_forecast():
    assert forecast(get_profile("LETTUCE"), series(soilMoisture=(75, -1))[:4]) == []


def test_light_is_not_forecast():
    assert "brightness" not in by_parameter(forecast(get_profile("LETTUCE"), series(brightness=(900, -50))))


def test_agent_waters_before_the_substrate_dries():
    profile = get_profile("LETTUCE")
    forecasts = forecast(profile, series(soilMoisture=(75, -1)))
    diagnosis = diagnose(profile, {"soilMoisture": 64})
    actions = decide(diagnosis, {}, {}, {"WATER_PUMP"}, forecasts=forecasts)
    assert [(a.actuator, a.action, a.duration_seconds) for a in actions] == [("WATER_PUMP", "ACTIVATE", 10)]
    assert actions[0].reason.startswith("Riego preventivo")


def test_agent_ventilates_before_it_gets_too_hot():
    profile = get_profile("LETTUCE")
    forecasts = forecast(profile, series(temperature=(18, 0.3)))
    diagnosis = diagnose(profile, {"temperature": 21.5})
    actions = decide(diagnosis, {}, {}, {"FAN"}, forecasts=forecasts)
    assert actions and actions[0].reason.startswith("Ventilación preventiva")

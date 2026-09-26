import pytest

from app.engine.fuzzy import health_index, memberships, parameter_health


def test_memberships_cover_the_whole_domain():
    for deviation in [0, 0.1, 0.3, 0.6, 0.8, 1.0, 1.5, 5]:
        assert sum(memberships(deviation).values()) > 0


def test_health_decreases_as_the_deviation_grows():
    scores = [parameter_health(d) for d in [0, 0.2, 0.5, 0.8, 1.1, 2.0]]
    assert scores == sorted(scores, reverse=True)
    assert scores[0] == pytest.approx(100)
    assert scores[-1] == pytest.approx(10)


def test_all_ideal_is_excellent():
    result = health_index({"ph": 0, "temperature": 0, "humidity": 0})
    assert result.index == 100
    assert result.level == "EXCELLENT"
    assert result.label == "Excelente"


def test_one_critical_variable_caps_the_index():
    result = health_index({"ph": 3.0, "temperature": 0, "humidity": 0, "tds": 0, "soilMoisture": 0, "brightness": 0})
    assert result.index <= 40
    assert result.level in {"POOR", "CRITICAL"}


def test_without_data_the_level_is_unknown():
    assert health_index({}).level == "UNKNOWN"

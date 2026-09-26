from app.engine.models import ModelRegistry, normalize
from app.knowledge.profiles import get_profile


def test_models_learn_the_synthetic_rules(models: ModelRegistry):
    assert models.metrics["ventilation_accuracy"] >= 0.85
    assert models.metrics["ph_correction_accuracy"] >= 0.85
    assert models.metrics["anomaly_detection_rate"] >= 0.9


def test_training_is_deterministic():
    assert ModelRegistry(seed=7).metrics == ModelRegistry(seed=7).metrics


def test_ventilation_is_likely_when_hot_and_humid(models: ModelRegistry):
    profile = get_profile("LETTUCE")
    hot = normalize(profile, {"temperature": 30, "humidity": 85})
    cool = normalize(profile, {"temperature": 18, "humidity": 60})
    assert models.ventilation_probability(hot) > 0.8
    assert models.ventilation_probability(cool) < 0.3


def test_ph_correction_detects_both_sides_of_the_range(models: ModelRegistry):
    profile = get_profile("TOMATO")
    assert models.ph_correction_probability(normalize(profile, {"ph": 4.5, "tds": 1800})) > 0.7
    assert models.ph_correction_probability(normalize(profile, {"ph": 7.6, "tds": 1800})) > 0.7
    assert models.ph_correction_probability(normalize(profile, {"ph": 6.0, "tds": 1800})) < 0.3


def test_missing_inputs_skip_the_prediction(models: ModelRegistry):
    assert models.ventilation_probability({"temperature": 0.5}) is None
    assert models.ph_correction_probability({}) is None


def test_sudden_spikes_are_flagged_as_anomalies(models: ModelRegistry):
    profile = get_profile("BASIL")
    history = [normalize(profile, {"temperature": 25 + (i % 3) * 0.2, "humidity": 50}) for i in range(20)]
    normal = normalize(profile, {"temperature": 25.3, "humidity": 50})
    spike = normalize(profile, {"temperature": 58, "humidity": 50})
    assert models.anomaly_probability(normal, history) < 0.5
    assert models.anomaly_probability(spike, history) >= 0.8

from app.engine.diagnosis import diagnose
from app.engine.expert_system import InferenceEngine, Rule, WorkingMemory
from app.engine.rules import RULES
from app.knowledge.profiles import get_profile


def run(measures: dict, crop: str = "LETTUCE", **predictions) -> tuple[list[str], WorkingMemory]:
    memory = WorkingMemory()
    for item in diagnose(get_profile(crop), measures):
        memory.assert_fact(f"status:{item.parameter}", item.status)
        memory.assert_fact(f"severity:{item.parameter}", item.severity)
    for name, value in predictions.items():
        memory.assert_fact(f"prediction:{name}", value)
    firings = InferenceEngine(RULES).run(memory)
    return [f.rule for f in firings], memory


def test_engine_chains_conclusions_and_fires_each_rule_once():
    rules = [
        Rule("a", "A", condition=lambda m: m.has("start"), message=lambda m: "a", conclude=lambda m: {"b": True}),
        Rule("b", "B", condition=lambda m: m.has("b"), message=lambda m: "b", salience=-1),
    ]
    memory = WorkingMemory({"start": True})

    firings = InferenceEngine(rules).run(memory)

    assert [f.rule for f in firings] == ["a", "b"]
    assert memory.get("rule:a") == 1.0


def test_ideal_conditions_when_everything_is_in_range():
    fired, _ = run({"temperature": 18, "humidity": 60, "brightness": 800, "ph": 6.0, "tds": 700, "soilMoisture": 70})
    assert fired == ["ideal_conditions"]


def test_heat_stress_and_compound_stress_are_chained():
    fired, memory = run({"temperature": 30, "humidity": 40, "ph": 7.2, "tds": 700})
    assert "heat_stress" in fired
    assert "nutrient_lockout" in fired
    assert "compound_stress" in fired
    assert memory.get("stress") == "heat"


def test_critical_state_needs_two_critical_variables():
    fired, _ = run({"temperature": 35, "soilMoisture": 20, "humidity": 60})
    assert "critical_state" in fired
    assert "drought" in fired


def test_sensor_fault_suppresses_critical_state():
    fired, memory = run({"temperature": 59, "soilMoisture": 0, "ph": 0.1}, anomaly=0.95)
    assert "sensor_fault" in fired
    assert "critical_state" not in fired
    assert memory.get("sensor_fault") is True


def test_diagnosis_messages_are_spanish_and_ranked():
    items = {d.parameter: d for d in diagnose(get_profile("TOMATO"), {"ph": 7.8, "tds": 1500})}
    assert items["ph"].status == "HIGH"
    assert items["ph"].severity == "CRITICAL"
    assert items["ph"].message.startswith("El pH")
    assert "pH Down" in items["ph"].recommendation
    assert items["tds"].status == "OPTIMAL"

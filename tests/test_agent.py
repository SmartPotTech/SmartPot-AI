from app.engine.agent import decide
from app.engine.diagnosis import diagnose
from app.knowledge.profiles import get_profile

ALL = {"WATER_PUMP", "UV_LIGHT", "FAN", "HUMIDIFIER", "PH_DOSER", "NUTRIENT_DOSER"}


def actions_for(measures: dict, actuators=ALL, predictions=None, facts=None):
    diagnosis = diagnose(get_profile("LETTUCE"), measures)
    return decide(diagnosis, predictions or {}, facts or {}, set(actuators))


def test_dry_substrate_triggers_irrigation():
    actions = actions_for({"soilMoisture": 30})
    assert [(a.actuator, a.action, a.duration_seconds) for a in actions] == [("WATER_PUMP", "ACTIVATE", 30)]


def test_only_available_actuators_are_proposed():
    actions = actions_for({"soilMoisture": 30, "humidity": 30, "temperature": 30}, actuators={"FAN"})
    assert [a.actuator for a in actions] == ["FAN"]


def test_light_is_turned_on_or_off_depending_on_the_level():
    assert actions_for({"brightness": 100})[0].action == "ACTIVATE"
    assert actions_for({"brightness": 1900})[0].action == "DEACTIVATE"


def test_ventilation_prediction_alone_can_start_the_fan():
    actions = actions_for({"temperature": 21, "humidity": 69}, predictions={"ventilation": 0.9})
    assert any(a.actuator == "FAN" and a.action == "ACTIVATE" for a in actions)


def test_no_nutrients_while_the_ph_blocks_absorption():
    actions = actions_for({"tds": 300, "ph": 7.5}, facts={"lockout": True})
    assert all(a.actuator != "NUTRIENT_DOSER" for a in actions)


def test_suspected_sensor_fault_stops_all_actions():
    assert actions_for({"soilMoisture": 0, "temperature": 59}, facts={"sensor_fault": True}) == []


def test_ideal_conditions_need_no_action():
    measures = {"temperature": 18, "humidity": 60, "brightness": 800, "ph": 6.0, "tds": 700, "soilMoisture": 70}
    assert actions_for(measures) == []

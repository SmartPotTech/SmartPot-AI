from app.engine.fleet import CropInput, analyze_fleet

IDEAL_LETTUCE = {"temperature": 18, "humidity": 60, "brightness": 900, "ph": 6.0, "tds": 700, "soilMoisture": 70}
ALL = {"WATER_PUMP", "UV_LIGHT", "FAN"}


def crop(crop_id: str, crop_type: str = "LETTUCE", actuators=ALL, **changes) -> CropInput:
    measures = {**IDEAL_LETTUCE, **changes} if crop_type == "LETTUCE" else changes
    return CropInput(crop_id, f"Cultivo {crop_id}", crop_type, measures, set(actuators))


def test_ranking_puts_the_crop_that_needs_attention_first():
    result = analyze_fleet([crop("a"), crop("b", soilMoisture=20, temperature=30)])
    ranked = sorted((c for c in result.crops if c.rank), key=lambda c: c.rank)
    assert [c.id for c in ranked] == ["b", "a"]
    assert result.average_health is not None
    assert "«Cultivo b»" in result.summary


def test_the_same_problem_in_most_crops_is_an_environment_issue():
    result = analyze_fleet([crop("a", temperature=28), crop("b", temperature=29), crop("c")])
    issue = next(i for i in result.shared_issues if i.parameter == "temperature")
    assert issue.status == "HIGH"
    assert sorted(issue.crop_ids) == ["a", "b"]
    assert "entorno" in issue.message


def test_a_single_crop_problem_is_not_shared():
    result = analyze_fleet([crop("a", temperature=28), crop("b"), crop("c")])
    assert all(i.parameter != "temperature" for i in result.shared_issues)


def test_crops_are_grouped_by_similar_conditions():
    crops = [crop("hot1", temperature=29, soilMoisture=35), crop("hot2", temperature=30, soilMoisture=38),
             crop("ok1"), crop("ok2", temperature=19)]
    groups = {tuple(sorted(g.crop_ids)): g for g in analyze_fleet(crops).groups}
    assert ("hot1", "hot2") in groups
    assert ("ok1", "ok2") in groups
    assert groups[("ok1", "ok2")].label == "En rango"
    assert "temperatura alta" in groups[("hot1", "hot2")].label.lower()


def test_actions_are_merged_by_actuator():
    result = analyze_fleet([crop("a", soilMoisture=30), crop("b", soilMoisture=40), crop("c"),
                            crop("d", soilMoisture=30, actuators={"FAN"})])
    watering = next(a for a in result.actions if a.actuator == "WATER_PUMP")
    assert sorted(watering.crop_ids) == ["a", "b"]
    assert watering.duration_seconds == 30


def test_crops_without_readings_are_listed_without_health():
    result = analyze_fleet([CropInput("empty", "Sin lecturas", "TOMATO", None, set()), crop("a")])
    empty = next(c for c in result.crops if c.id == "empty")
    assert empty.health is None and empty.rank == 0

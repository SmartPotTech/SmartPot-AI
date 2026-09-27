import numpy as np
import pytest

from app.engine import agent
from app.engine.diagnosis import diagnose
from app.knowledge.profiles import get_profile
from app.learning.features import build, live_features
from app.learning.quality import assess
from app.learning.service import LearningConfig, LearningService, fit_type
from app.learning.store import ReadingStore
from tests.learning_data import START, lettuce_series


@pytest.fixture(scope="module")
def trained():
    store = ReadingStore(None)
    store.add(lettuce_series("a", seed=1) + lettuce_series("b", seed=2, start=START + 90))
    return fit_type("LETTUCE", store.load("LETTUCE", 12_000), 200, None)


def test_store_ignores_repeated_readings_and_forgets_a_crop():
    store = ReadingStore(None)
    readings = lettuce_series("crop-1", days=0.1)
    assert store.add(readings) == len(readings)
    assert store.add(readings[:5]) == 0
    assert store.counts()["LETTUCE"]["crops"] == 1
    assert store.forget("crop-1") == len(readings)
    assert store.total() == 0


def test_store_keeps_the_crop_id_pseudonymized(tmp_path):
    store = ReadingStore(tmp_path)
    store.add(lettuce_series("66f5a1000000000000000101", days=0.05))
    series = store.load("LETTUCE", 100)
    assert store.persistent
    assert "66f5a1000000000000000101" not in set(series.crops)


def test_prune_applies_the_row_cap_per_species():
    store = ReadingStore(None)
    store.add(lettuce_series("a", days=0.5, start=4_000_000_000.0))
    store.prune(retention_days=36_500, max_rows_per_type=50)
    assert store.total() == 50


def test_quality_flags_physically_impossible_spikes():
    values = np.tile([20.0, 60, 900, 6.0, 700, 1012, 70], (200, 1)) + np.random.default_rng(0).normal(0, 0.1, (200, 7))
    values[10, 0] = 95.0
    report = assess(values)
    assert not report.keep[10]
    assert report.keep.sum() == 199
    assert report.validity < 1.0


def test_labels_come_from_what_happened_in_the_next_hour():
    profile = get_profile("LETTUCE")
    store = ReadingStore(None)
    store.add(lettuce_series("a", days=1))
    dataset = build(profile, store.load("LETTUCE", 10_000))
    soil = dataset.states[:, 5]
    # Una lectura cerca del mínimo con el sustrato secándose termina etiquetada como «necesitará riego».
    near_min = dataset.water_mask & (soil < 0.15)
    assert dataset.water[near_min].mean() > 0.8
    assert dataset.water[dataset.water_mask & (soil > 0.8)].mean() < 0.05
    assert np.nanmax(np.abs(dataset.moisture_delta)) > 5


def test_supervised_models_beat_the_baseline_on_recent_readings(trained):
    water = trained.cards["needs_water"]
    assert water.status == "TRAINED"
    assert water.holdout > water.baseline + 0.2
    assert {c.model for c in water.candidates} >= {"Línea base", "Regresión logística", "Bosque aleatorio"}
    moisture = trained.cards["moisture_1h"]
    assert moisture.status == "TRAINED"
    assert moisture.holdout < moisture.baseline


def test_unsupervised_models_find_operating_states_and_rare_readings(trained):
    assert trained.states.k >= 2
    assert abs(sum(c.share for c in trained.states.clusters) - 1) < 0.01
    typical = np.array([[0.4, 0.5, 0.5, 0.5, 0.5, 0.7]])
    extreme = np.array([[4.0, -3.0, 5.0, 4.0, -2.0, -3.0]])
    assert not trained.anomaly.score(typical)[1]
    assert trained.anomaly.score(extreme)[1]


def test_rare_situations_wait_for_more_cases():
    store = ReadingStore(None)
    store.add(lettuce_series("a", days=0.6))
    state = fit_type("LETTUCE", store.load("LETTUCE", 10_000), 200, None)
    assert state.cards["overheat"].status == "PENDING"
    assert "casos" in state.cards["overheat"].reason or "Faltan" in state.cards["overheat"].reason


def test_the_current_champion_stays_when_the_challenger_is_not_better(trained):
    store = ReadingStore(None)
    store.add(lettuce_series("a", seed=1) + lettuce_series("b", seed=2, start=START + 90))
    again = fit_type("LETTUCE", store.load("LETTUCE", 12_000), 200, trained)
    card = again.cards["needs_water"]
    assert card.version >= trained.cards["needs_water"].version
    assert card.usable


def test_service_trains_persists_and_reloads(tmp_path):
    config = LearningConfig(data_dir=str(tmp_path), check_seconds=0, separate_process=False)
    service = LearningService(config)
    service.ingest(lettuce_series("a") + lettuce_series("b", seed=2, start=START + 90))
    assert service.due() == ["LETTUCE"]
    service.train("LETTUCE")
    assert service.due() == []
    assert (tmp_path / "models" / "LETTUCE.joblib").exists()
    service.stop()

    reloaded = LearningService(config)
    assert reloaded.types["LETTUCE"].cards["needs_water"].usable
    assert reloaded.status()["crop_types"][0]["readings"] == 2160
    reloaded.stop()


def test_live_features_use_the_recent_trend():
    profile = get_profile("LETTUCE")
    history = [(START + i * 300, {"soilMoisture": 72 - i, "temperature": 18.0}) for i in range(6)]
    features = live_features(profile, {"temperature": 18, "humidity": 60, "brightness": 800, "ph": 6.0,
                                       "tds": 700, "soilMoisture": 66}, 14, START + 1800, history)
    assert features.shape == (1, 10)
    assert features[0, 8] < 0


def test_agent_waters_preventively_when_the_learned_model_is_confident():
    profile = get_profile("LETTUCE")
    diagnosis = diagnose(profile, {"temperature": 18, "humidity": 60, "brightness": 800, "ph": 6.0, "tds": 700,
                                   "soilMoisture": 64}, 14)
    actions = agent.decide(diagnosis, {}, {"learned:needs_water": 0.9}, {"WATER_PUMP"})
    assert [(a.actuator, a.duration_seconds) for a in actions] == [("WATER_PUMP", 10)]
    assert "aprendido" in actions[0].reason
    assert agent.decide(diagnosis, {}, {"learned:needs_water": 0.6}, {"WATER_PUMP"}) == []


def test_ingest_endpoint_validates_and_reports(client, auth):
    body = {"readings": [{"cropId": "c1", "cropType": "LETTUCE", "measuredAt": "2026-09-27T12:00:00Z",
                          "localHour": 7, "measures": {"temperature": 19, "soilMoisture": 70}}]}
    assert client.post("/v1/learning/readings", json=body).status_code == 401
    response = client.post("/v1/learning/readings", json=body, headers=auth)
    assert response.status_code == 200
    assert response.json()["stored"] == 1
    bad = {"readings": [{**body["readings"][0], "cropType": "CACTUS"}]}
    assert client.post("/v1/learning/readings", json=bad, headers=auth).status_code == 422
    assert client.delete("/v1/learning/crops/c1", headers=auth).json() == {"deleted": 1}


def test_insights_report_what_was_learned(client, auth):
    readings = [
        {"cropId": r.crop_id, "cropType": "LETTUCE", "measuredAt": r.measured_at, "localHour": r.local_hour,
         "measures": {"temperature": r.values["temperature"], "humidity": r.values["humidity"],
                      "brightness": r.values["brightness"], "ph": r.values["ph"], "tds": r.values["tds"],
                      "atmosphere": r.values["atmosphere"], "soilMoisture": r.values["soil_moisture"]}}
        for r in lettuce_series("api-a") + lettuce_series("api-b", seed=2, start=START + 90)
    ]
    before = client.post("/v1/insights", json={"cropType": "LETTUCE", "measures": {"soilMoisture": 70}},
                         headers=auth).json()
    assert before["learning"]["source"] == "BASE"

    for start in range(0, len(readings), 2000):
        client.post("/v1/learning/readings", json={"readings": readings[start:start + 2000]}, headers=auth)
    status = client.post("/v1/learning/train", json={"cropType": "LETTUCE"}, headers=auth).json()
    lettuce = next(item for item in status["cropTypes"] if item["cropType"] == "LETTUCE")
    assert {m["task"] for m in lettuce["models"]} == {"needs_water", "overheat", "moisture_1h"}
    assert lettuce["quality"]["score"] > 0.9

    history = [{"temperature": 21, "humidity": 58, "brightness": 1200, "ph": 6.0, "tds": 700,
                "soilMoisture": 64 - i * 0.6, "measuredAt": f"2026-09-27T17:{i * 5:02d}:00Z"} for i in range(7)]
    response = client.post("/v1/insights", headers=auth, json={
        "cropType": "LETTUCE", "localHour": 13, "history": history,
        "measures": {"temperature": 21, "humidity": 58, "brightness": 1200, "ph": 6.0, "tds": 700,
                     "soilMoisture": 60.5, "measuredAt": "2026-09-27T17:35:00Z"},
    }).json()
    learning = response["learning"]
    assert learning["source"] == "LEARNED"
    assert learning["state"]["label"]
    assert "expectedIn1h" in learning["moisture"]
    water = next(p for p in learning["predictions"] if p["name"] == "needs_water")
    assert water["probability"] > 0.5

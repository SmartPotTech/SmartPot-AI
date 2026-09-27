PAYLOAD = {
    "cropType": "LETTUCE",
    "measures": {"temperature": 29, "humidity": 42, "brightness": 900, "ph": 7.3, "tds": 700, "soilMoisture": 35},
    "history": [{"temperature": 28 + (i % 4) * 0.4, "humidity": 44 - (i % 3), "ph": 7.1 + (i % 2) * 0.1,
                 "soilMoisture": 38 - (i % 4)} for i in range(12)],
    "actuators": ["WATER_PUMP", "UV_LIGHT", "FAN"],
}


def test_health_is_public(client):
    response = client.get("/health")
    assert response.status_code == 200
    assert response.json() == {"status": "UP", "models": "READY", "learning": "PERSISTENT"}


def test_insights_require_the_service_token(client):
    assert client.post("/v1/insights", json=PAYLOAD).status_code == 401
    wrong = client.post("/v1/insights", json=PAYLOAD, headers={"Authorization": "Bearer otro"})
    assert wrong.status_code == 401
    assert wrong.json()["detail"] == "Token de servicio inválido"


def test_insights_combine_all_the_techniques(client, auth):
    response = client.post("/v1/insights", json=PAYLOAD, headers=auth)

    assert response.status_code == 200
    body = response.json()
    assert body["cropType"] == "LETTUCE"
    assert 0 <= body["health"]["index"] < 70
    assert {d["parameter"] for d in body["diagnosis"]} == {"temperature", "humidity", "brightness", "ph", "tds",
                                                           "soilMoisture"}
    assert {c["rule"] for c in body["conclusions"]} >= {"heat_stress", "nutrient_lockout"}
    assert {p["name"] for p in body["predictions"]} == {"ventilation", "ph_correction", "anomaly"}
    assert {a["actuator"] for a in body["actions"]} == {"WATER_PUMP", "FAN"}
    assert all("durationSeconds" in a for a in body["actions"])
    assert body["summary"].startswith("Tu lechuga")


def test_sudden_jumps_are_treated_as_possible_sensor_faults(client, auth):
    steady = [{"temperature": 20 + (i % 3) * 0.1, "humidity": 60, "ph": 6.0, "soilMoisture": 70} for i in range(15)]
    payload = {**PAYLOAD, "history": steady}

    body = client.post("/v1/insights", json=payload, headers=auth).json()

    assert "sensor_fault" in {c["rule"] for c in body["conclusions"]}
    assert body["actions"] == []


def test_unknown_crop_types_are_rejected_in_spanish(client, auth):
    response = client.post("/v1/insights", json={**PAYLOAD, "cropType": "MINT"}, headers=auth)
    assert response.status_code == 422
    assert response.json()["detail"] == "Datos inválidos"


def test_out_of_range_values_are_rejected(client, auth):
    payload = {**PAYLOAD, "measures": {"ph": 20}}
    assert client.post("/v1/insights", json=payload, headers=auth).status_code == 422


def test_crop_profiles_are_exposed_to_the_api(client, auth):
    response = client.get("/v1/crop-profiles", headers=auth)
    assert response.status_code == 200
    lettuce = next(p for p in response.json() if p["type"] == "LETTUCE")
    assert lettuce["name"] == "Lechuga"
    assert lettuce["ranges"]["soilMoisture"] == {"min": 60.0, "max": 80.0, "unit": "%",
                                                 "label": "la humedad del sustrato"}


def test_model_metrics_are_reported(client, auth):
    metrics = client.get("/v1/models", headers=auth).json()
    assert set(metrics) == {"ventilation_accuracy", "ph_correction_accuracy", "anomaly_detection_rate"}


def test_docs_are_hidden_by_default(client):
    assert client.get("/docs").status_code == 404
    assert client.get("/openapi.json").status_code == 404


def test_the_local_hour_enables_the_night_rest(client, auth):
    payload = {"cropType": "TOMATO", "localHour": 23, "actuators": ["UV_LIGHT"],
               "measures": {"temperature": 21, "humidity": 70, "brightness": 20, "ph": 6.2, "tds": 2000,
                            "soilMoisture": 65}}
    body = client.post("/v1/insights", json=payload, headers=auth).json()

    assert next(d for d in body["diagnosis"] if d["parameter"] == "brightness")["status"] == "REST"
    assert body["health"]["index"] >= 85
    assert body["actions"] == []
    assert "night_rest" in {c["rule"] for c in body["conclusions"]}


def test_the_local_hour_must_be_a_valid_hour(client, auth):
    payload = {"cropType": "TOMATO", "localHour": 24, "measures": {"brightness": 20}}
    assert client.post("/v1/insights", json=payload, headers=auth).status_code == 422


def test_timed_history_produces_forecasts_and_explanations(client, auth):
    history = [{"soilMoisture": 75 - step, "temperature": 19.0, "measuredAt": f"2026-09-26T12:{step * 5:02d}:00Z"}
               for step in range(12)]
    payload = {"cropType": "LETTUCE", "localHour": 12, "actuators": ["WATER_PUMP"], "history": history,
               "measures": {"temperature": 19, "humidity": 60, "brightness": 900, "ph": 6.0, "tds": 700,
                            "soilMoisture": 64}}
    body = client.post("/v1/insights", json=payload, headers=auth).json()

    soil = next(f for f in body["forecasts"] if f["parameter"] == "soilMoisture")
    assert soil["trend"] == "FALLING" and soil["limit"] == "MIN"
    assert set(soil) >= {"slopePerHour", "expectedIn3h", "hoursToLimit", "confidence"}
    assert "drying_trend" in {c["rule"] for c in body["conclusions"]}
    assert body["actions"][0]["reason"].startswith("Riego preventivo")
    assert set(body["health"]["byParameter"]) >= {"temperature", "soilMoisture"}


def test_fleet_analysis_compares_all_crops(client, auth):
    ideal = {"temperature": 18, "humidity": 60, "brightness": 900, "ph": 6.0, "tds": 700, "soilMoisture": 70}
    payload = {"localHour": 12, "crops": [
        {"id": "a", "name": "Lechuga 1", "cropType": "LETTUCE", "measures": {**ideal, "temperature": 28},
         "actuators": ["FAN"]},
        {"id": "b", "name": "Lechuga 2", "cropType": "LETTUCE", "measures": {**ideal, "temperature": 29},
         "actuators": ["FAN"]},
        {"id": "c", "name": "Tomate", "cropType": "tomato"},
    ]}
    response = client.post("/v1/fleet", json=payload, headers=auth)

    assert response.status_code == 200
    body = response.json()
    assert body["crops"][2]["health"] is None
    assert body["sharedIssues"][0]["parameter"] == "temperature"
    assert body["actions"][0]["actuator"] == "FAN" and sorted(body["actions"][0]["cropIds"]) == ["a", "b"]
    assert body["crops"][0]["issues"] == ["Temperatura alta"]
    assert client.post("/v1/fleet", json={"crops": []}, headers=auth).status_code == 422

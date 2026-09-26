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
    assert response.json() == {"status": "UP", "models": "READY"}


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

"""Generate the learned-model parity cases: Trainer models evaluated by Rust and by the Python service.

Step 1, with the SmartPot-Trainer environment: trains lettuce on synthetic data, exports `LETTUCE_inference.json`
signed with a fixed test key and stores the state for step 2.

    uv run --project ../SmartPot-Trainer python tools/generate_learned_fixtures.py train ../SmartPot-Trainer

Step 2, with the SmartPot-AI environment: loads those models into the Python service and stores its responses.
The Python service answers in Spanish; the expected texts are regenerated in English with
`SMARTPOT_REGENERATE_FIXTURES=1 cargo test --test regenerate -- --ignored` (numbers and structure stay unchanged).

    uv run --project ../SmartPot-AI python tools/generate_learned_fixtures.py expect ../SmartPot-AI
"""

import json
import os
import random
import sys
from datetime import UTC, datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "tests" / "fixtures" / "learned"
WORK = ROOT / "target" / "learned-fixtures"
TOKEN = "parity-token-for-local-tests-only"
# Clave Ed25519 solo para pruebas: 32 bytes fijos.
TEST_KEY = bytes(range(32))
ACTUATORS = ["WATER_PUMP", "UV_LIGHT", "FAN", "HUMIDIFIER", "PH_DOSER", "NUTRIENT_DOSER"]
CAMEL = {"soil_moisture": "soilMoisture"}


def iso(seconds: float) -> str:
    return datetime.fromtimestamp(seconds, UTC).isoformat().replace("+00:00", "Z")


def measures(values: dict) -> dict:
    return {CAMEL.get(k, k): v for k, v in values.items() if v is not None}


def train(trainer: Path) -> None:
    sys.path.insert(0, str(trainer))
    import joblib  # noqa: PLC0415
    import sklearn  # noqa: PLC0415
    from cryptography.hazmat.primitives.asymmetric import ed25519  # noqa: PLC0415

    from app.learning.exporter import export_crop_models  # noqa: PLC0415
    from app.learning.service import fit_type  # noqa: PLC0415
    from app.learning.store import ReadingStore  # noqa: PLC0415
    from tests.learning_data import lettuce_series  # noqa: PLC0415

    readings = lettuce_series("crop-a", days=3.0) + lettuce_series("crop-b", days=3.0, seed=2)
    store = ReadingStore(None)
    store.add(readings)
    state = fit_type("LETTUCE", store.load("LETTUCE", 12_000), 100, None)
    store.close()

    TARGET.mkdir(parents=True, exist_ok=True)
    for old in TARGET.glob("*"):
        old.unlink()
    key = ed25519.Ed25519PrivateKey.from_private_bytes(TEST_KEY)
    export_crop_models(TARGET, "LETTUCE", state, key, min_samples=100, onnx=False)
    for extra in ("manifest.json", "LETTUCE.joblib"):
        (TARGET / extra).unlink(missing_ok=True)
    public = key.public_key().public_bytes_raw().hex()
    (TARGET / "trusted_key.txt").write_text(public, encoding="utf-8")

    rnd = random.Random(7)
    series = [r for r in readings if r.crop_id == "crop-b"]
    requests = []
    for index in sorted(rnd.sample(range(20, len(series)), 40)):
        current = series[index]
        values = dict(current.values)
        pick = rnd.random()
        if pick < 0.2:
            values["soil_moisture"] = round(rnd.uniform(58, 62), 1)
        elif pick < 0.35:
            values["temperature"] = round(rnd.uniform(20, 23), 1)
        elif pick < 0.45:
            values.pop("ph")
        body = {
            "cropType": "LETTUCE",
            "measures": {**measures(values), "measuredAt": iso(current.measured_at)},
            "history": [
                {**measures(r.values), "measuredAt": iso(r.measured_at)} for r in series[index - rnd.choice([3, 8, 12]):index]
            ],
            "actuators": ACTUATORS,
            "localHour": current.local_hour,
        }
        if rnd.random() < 0.3:
            body["placement"] = {"setting": "OUTDOOR", "exposure": rnd.choice(["FULL_SUN", "PARTIAL_SUN"])}
        requests.append(body)

    WORK.mkdir(parents=True, exist_ok=True)
    joblib.dump({"sklearn": sklearn.__version__, "state": state}, WORK / "LETTUCE.joblib")
    (WORK / "requests.json").write_text(json.dumps(requests), encoding="utf-8")
    print(f"Models in {TARGET}; {len(requests)} requests; training readings: {state.readings_at_training}")


def expect(ai: Path) -> None:
    models = WORK / "ai-data"
    (models / "models").mkdir(parents=True, exist_ok=True)
    (models / "models" / "LETTUCE.joblib").write_bytes((WORK / "LETTUCE.joblib").read_bytes())
    os.environ.update({"SMARTPOT_AI_TOKEN": TOKEN, "DATA_DIR": str(models), "LEARNING_CHECK_SECONDS": "0"})
    os.chdir(ai)
    sys.path.insert(0, str(ai))
    from fastapi.testclient import TestClient  # noqa: PLC0415

    from app.main import create_app  # noqa: PLC0415

    bundle = json.loads((TARGET / "LETTUCE_inference.json").read_text(encoding="utf-8"))
    readings = bundle["readings_at_training"]
    requests = json.loads((WORK / "requests.json").read_text(encoding="utf-8"))
    cases = []
    with TestClient(create_app()) as client:
        service = client.app.state.learning
        assert "LETTUCE" in service.types, "the Python service did not load the Trainer models"
        # Sin Trainer, la IA en Rust informa las lecturas con las que se entrenó: el oráculo hace lo mismo.
        service.counts = lambda: {"LETTUCE": {"readings": readings, "crops": 2}}
        for body in requests:
            response = client.post("/v1/insights", json=body, headers={"Authorization": f"Bearer {TOKEN}"})
            cases.append({"request": body, "status": response.status_code, "response": response.json()})
    (TARGET / "expected_insights.json").write_text(json.dumps(cases, ensure_ascii=False, indent=1), encoding="utf-8")
    learned = sum(1 for c in cases if c["response"].get("learning", {}).get("predictions"))
    print(f"{len(cases)} responses; {learned} with learned predictions")


if __name__ == "__main__":
    mode, project = sys.argv[1], Path(sys.argv[2]).resolve()
    {"train": train, "expect": expect}[mode](project)

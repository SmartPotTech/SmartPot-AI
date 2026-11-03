"""Genera los casos de paridad entre SmartPot-AI (Python) y SmartPot-IA-Rust.

Arma peticiones variadas (todas las especies, valores dentro y fuera de rango, sensores ausentes, ceros de noche,
historial con tendencia, lugar, clima, actuadores y peticiones inválidas), las envía al servicio en Python y guarda la
petición con su respuesta exacta. Las pruebas de Rust envían las mismas peticiones y exigen la misma respuesta.

Uso, desde la raíz de SmartPot-IA-Rust y con el entorno de SmartPot-AI:

    uv run --project ../SmartPot-AI python tools/generate_parity_fixtures.py ../SmartPot-AI
"""

import json
import os
import random
import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOKEN = "parity-token-for-local-tests-only"
CROPS = ("LETTUCE", "TOMATO", "STRAWBERRY", "BASIL", "SPINACH", "PEPPER")
ACTUATORS = ("WATER_PUMP", "UV_LIGHT", "FAN", "HUMIDIFIER", "PH_DOSER", "NUTRIENT_DOSER")
LIMITS = {
    "temperature": (-20.0, 60.0),
    "humidity": (0.0, 100.0),
    "brightness": (0.0, 200_000.0),
    "ph": (0.0, 14.0),
    "tds": (0.0, 10_000.0),
    "atmosphere": (300.0, 1_100.0),
    "soilMoisture": (0.0, 100.0),
}
ORIGIN = datetime(2026, 10, 1, 12, 0, tzinfo=UTC)


def iso(moment: datetime, rnd: random.Random) -> str:
    style = rnd.random()
    if style < 0.6:
        return moment.isoformat().replace("+00:00", "Z")
    if style < 0.85:
        return moment.astimezone(datetime.now(UTC).astimezone().tzinfo).isoformat()
    return moment.replace(tzinfo=None).isoformat()


def value(rnd: random.Random, parameter: str, rng: dict) -> float | None:
    low, high = LIMITS[parameter]
    if parameter == "atmosphere":
        return round(rnd.uniform(700, 1050), 1)
    minimum, maximum, tolerance = rng["min"], rng["max"], rng["tolerance"]
    pick = rnd.random()
    if pick < 0.45:
        raw = rnd.uniform(minimum, maximum)
    elif pick < 0.7:
        raw = rnd.choice([minimum - rnd.uniform(0, tolerance), maximum + rnd.uniform(0, tolerance)])
    elif pick < 0.9:
        raw = rnd.choice([minimum - rnd.uniform(tolerance, 3 * tolerance), maximum + rnd.uniform(tolerance, 3 * tolerance)])
    else:
        raw = rnd.choice([minimum, maximum, 0.0, low])
    raw = min(high, max(low, raw))
    return round(raw, rnd.choice([0, 1, 2, 6]))


def measures(rnd: random.Random, profile: dict, missing: float = 0.12) -> dict:
    result = {}
    for parameter in LIMITS:
        if rnd.random() < missing:
            continue
        rng = profile.get(parameter)
        if rng is None:
            result[parameter] = round(rnd.uniform(700, 1050), 1)
        else:
            result[parameter] = value(rnd, parameter, rng)
    return result


def history(rnd: random.Random, profile: dict, current: dict) -> list[dict]:
    size = rnd.choice([0, 0, 3, 6, 8, 12, 20, 40, 80])
    step = timedelta(minutes=rnd.choice([1, 5, 10, 15, 30]))
    slopes = {p: rnd.uniform(-1, 1) * (profile[p]["max"] - profile[p]["min"]) / 12 for p in profile}
    points = []
    for i in range(size):
        moment = ORIGIN - step * (size - i)
        item = {}
        if rnd.random() > 0.05:
            item["measuredAt"] = iso(moment, rnd)
        for parameter in LIMITS:
            if rnd.random() < 0.08:
                continue
            base = current.get(parameter)
            if base is None:
                base = (profile[parameter]["min"] + profile[parameter]["max"]) / 2 if parameter in profile else 900.0
            hours = (size - i) * step.total_seconds() / 3600
            drift = slopes.get(parameter, 0.0) * hours
            noise = rnd.gauss(0, 0.02 * abs(base) + 0.01)
            if rnd.random() < 0.03:
                noise *= 40
            low, high = LIMITS[parameter]
            item[parameter] = round(min(high, max(low, base - drift + noise)), 2)
        points.append(item)
    if points and rnd.random() < 0.1:
        points.append(dict(points[-1]))
    rnd.shuffle(points) if rnd.random() < 0.2 else None
    return points


def insight_request(rnd: random.Random, profiles: dict) -> dict:
    crop = rnd.choice(CROPS)
    profile = profiles[crop]
    current = measures(rnd, profile)
    request = {"cropType": crop if rnd.random() > 0.2 else crop.lower(), "measures": current}
    if rnd.random() < 0.15:
        current["measuredAt"] = iso(ORIGIN, rnd)
    if rnd.random() < 0.2 and "brightness" in current:
        current["brightness"] = 0.0
    if rnd.random() < 0.1 and "temperature" in current:
        current["temperature"] = 0.0
    points = history(rnd, profile, current)
    if points:
        request["history"] = points
    if rnd.random() < 0.85:
        chosen = [a for a in ACTUATORS if rnd.random() < 0.6]
        request["actuators"] = [a.lower() if rnd.random() < 0.1 else a for a in chosen]
    if rnd.random() < 0.8:
        request["localHour"] = rnd.choice([0, 3, 5, 6, 7, 12, 15, 18, 21, 22, 23, rnd.randint(0, 23)])
    if rnd.random() < 0.6:
        place = {}
        if rnd.random() < 0.9:
            place["setting"] = rnd.choice(["INDOOR", "OUTDOOR"])
        if rnd.random() < 0.9:
            place["exposure"] = rnd.choice(["FULL_SUN", "PARTIAL_SUN", "SHADE"])
        request["placement"] = place
    if rnd.random() < 0.5:
        weather = {
            "temperature": round(rnd.uniform(-5, 38), 1),
            "humidity": round(rnd.uniform(10, 100), 0),
            "precipitation": rnd.choice([0, 0, 0.2, 0.5, 2.4, 12.0]),
            "radiation": rnd.choice([0, 120, 300, 399, 400, 650, 980]),
            "isDay": rnd.random() < 0.7,
        }
        if rnd.random() < 0.5:
            weather["condition"] = rnd.choice(["Despejado", "Lluvia", "Nublado"])
        if rnd.random() < 0.15:
            weather.pop("precipitation")
            weather.pop("radiation")
            weather.pop("isDay")
        request["weather"] = weather
    return request


INVALID_INSIGHTS = [
    {"cropType": "CACTUS", "measures": {"temperature": 20}},
    {"cropType": "LETTUCE", "measures": {"temperature": 61}},
    {"cropType": "LETTUCE", "measures": {"humidity": -1}},
    {"cropType": "LETTUCE", "measures": {"ph": 14.5}},
    {"cropType": "TOMATO", "measures": {"temperature": 20}, "localHour": 24},
    {"cropType": "TOMATO", "measures": {"temperature": 20}, "placement": {"setting": "ROOF"}},
    {"cropType": "TOMATO", "measures": {"temperature": 20}, "placement": {"exposure": "MOON"}},
    {"cropType": "BASIL", "measures": {"temperature": 20}, "weather": {"temperature": 20}},
    {"cropType": "BASIL", "measures": {"temperature": 20}, "weather": {"temperature": 70, "humidity": 10}},
    {"cropType": "BASIL", "measures": {"temperature": 20}, "weather": {"temperature": 20, "humidity": 10,
                                                                      "condition": "x" * 21}},
    {"cropType": "BASIL"},
    {"measures": {"temperature": 20}},
    {"cropType": "BASIL", "measures": {"temperature": 20}, "actuators": ["FAN"] * 21},
]


def _timeline(points: list[dict], minutes: int) -> list[dict]:
    start = ORIGIN - timedelta(minutes=minutes * len(points))
    return [{**p, "measuredAt": (start + timedelta(minutes=minutes * i)).isoformat().replace("+00:00", "Z")}
            for i, p in enumerate(points)]


STABLE = {"temperature": 18.0, "humidity": 60.0, "brightness": 800.0, "ph": 6.0, "tds": 700.0, "soilMoisture": 70.0}
# Casos dirigidos a reglas que el azar casi no alcanza.
SPECIAL_INSIGHTS = [
    {"cropType": "LETTUCE", "measures": {**STABLE, "temperature": 45.0, "ph": 10.0},
     "history": _timeline([dict(STABLE) for _ in range(12)], 5), "actuators": list(ACTUATORS), "localHour": 12},
    {"cropType": "TOMATO", "measures": {"temperature": 27.4, "humidity": 70.0},
     "history": _timeline([{"temperature": 22.0 + 0.6 * i, "humidity": 70.0} for i in range(9)], 15),
     "actuators": ["FAN"], "localHour": 13},
    {"cropType": "LETTUCE", "measures": {"soilMoisture": 61.0},
     "history": _timeline([{"soilMoisture": 75.0 - 1.7 * i} for i in range(8)], 10),
     "actuators": ["WATER_PUMP"], "localHour": 10},
    {"cropType": "BASIL", "measures": {"temperature": 30.0, "soilMoisture": 60.0},
     "placement": {"setting": "OUTDOOR", "exposure": "FULL_SUN"},
     "weather": {"temperature": 15.0, "humidity": 50.0, "radiation": 800.0, "isDay": True}, "localHour": 12},
    {"cropType": "PEPPER", "measures": {"soilMoisture": 20.0, "brightness": 100.0},
     "placement": {"setting": "OUTDOOR", "exposure": "SHADE"},
     "weather": {"temperature": 20.0, "humidity": 90.0, "precipitation": 3.0, "radiation": 350.0, "isDay": True},
     "actuators": list(ACTUATORS), "localHour": 9},
]


def fleet_request(rnd: random.Random, profiles: dict) -> dict:
    size = rnd.choice([1, 2, 2, 3, 3, 4, 5, 6, 8, 12])
    crops = []
    shared_dry = rnd.random() < 0.4
    for i in range(size):
        crop = rnd.choice(CROPS)
        item = {"id": f"66f5a10000000000000{i:05d}", "name": f"Cultivo {i + 1}", "cropType": crop,
                "actuators": [a for a in ACTUATORS if rnd.random() < 0.7]}
        if rnd.random() > 0.12:
            current = measures(rnd, profiles[crop], missing=0.1)
            if shared_dry and "soilMoisture" in current:
                current["soilMoisture"] = round(profiles[crop]["soilMoisture"]["min"] - rnd.uniform(2, 20), 1)
            item["measures"] = current
        crops.append(item)
    request = {"crops": crops}
    if rnd.random() < 0.8:
        request["localHour"] = rnd.randint(0, 23)
    return request


INVALID_FLEET = [
    {"crops": []},
    {"crops": [{"id": "", "name": "x", "cropType": "LETTUCE"}]},
    {"crops": [{"id": "a", "name": "x", "cropType": "CACTUS"}]},
    {"crops": [{"id": "a", "name": "x" * 81, "cropType": "LETTUCE"}]},
    {"crops": [{"id": str(i), "name": "x", "cropType": "LETTUCE"} for i in range(51)]},
    {"crops": [{"id": "a", "name": "x", "cropType": "LETTUCE"}], "localHour": -1},
]


def main() -> None:
    ai = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT.parent / "SmartPot-AI").resolve()
    os.environ.update({"SMARTPOT_AI_TOKEN": TOKEN, "DATA_DIR": "", "LEARNING_CHECK_SECONDS": "0"})
    os.chdir(ai)
    sys.path.insert(0, str(ai))
    from fastapi.testclient import TestClient  # noqa: PLC0415

    from app.knowledge.profiles import PROFILES  # noqa: PLC0415
    from app.main import create_app  # noqa: PLC0415

    profiles = {
        crop: {name: {"min": r.min, "max": r.max, "tolerance": r.tolerance} for name, r in profile.ranges.items()}
        for crop, profile in PROFILES.items()
    }
    rnd = random.Random(20261102)
    auth = {"Authorization": f"Bearer {TOKEN}"}
    with TestClient(create_app()) as client:
        insights = []
        for request in [insight_request(rnd, profiles) for _ in range(400)] + INVALID_INSIGHTS + SPECIAL_INSIGHTS:
            response = client.post("/v1/insights", json=request, headers=auth)
            insights.append({"request": request, "status": response.status_code, "response": response.json()})
        fleets = []
        for request in [fleet_request(rnd, profiles) for _ in range(150)] + INVALID_FLEET:
            response = client.post("/v1/fleet", json=request, headers=auth)
            fleets.append({"request": request, "status": response.status_code, "response": response.json()})
        extra = {
            "crop_profiles": client.get("/v1/crop-profiles", headers=auth).json(),
            "models": client.get("/v1/models", headers=auth).json(),
            "unauthorized": client.post("/v1/insights", json=INVALID_INSIGHTS[0]).json(),
        }

    target = ROOT / "tests" / "fixtures"
    target.mkdir(parents=True, exist_ok=True)
    for name, data in (("parity_insights", insights), ("parity_fleet", fleets), ("parity_extra", extra)):
        (target / f"{name}.json").write_text(json.dumps(data, ensure_ascii=False, indent=1), encoding="utf-8")
    ok = sum(1 for item in insights if item["status"] == 200)
    print(f"Evaluaciones: {len(insights)} ({ok} válidas); flotas: {len(fleets)}")


if __name__ == "__main__":
    main()

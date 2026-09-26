"""Índice de salud con lógica difusa (inferencia Sugeno de orden cero).

Entrada por variable: la desviación respecto al rango ideal, medida en tolerancias.
Conjuntos: «ideal», «aceptable», «desviada» y «crítica». Cada conjunto aporta un valor de
salud y el resultado de cada variable es el promedio ponderado por su grado de pertenencia.
El índice global pondera las variables por su importancia agronómica y aplica una regla de
peor caso: una variable crítica limita la salud total.
"""

from dataclasses import dataclass

WEIGHTS = {
    "ph": 1.3,
    "temperature": 1.2,
    "soilMoisture": 1.2,
    "tds": 1.1,
    "humidity": 0.9,
    "brightness": 0.8,
}

# Valor de salud que representa cada conjunto (consecuente de la regla).
OUTPUTS = {"ideal": 100.0, "acceptable": 75.0, "deviated": 45.0, "critical": 10.0}

LEVELS = (
    (85.0, "EXCELLENT", "Excelente"),
    (70.0, "GOOD", "Saludable"),
    (50.0, "FAIR", "Aceptable"),
    (30.0, "POOR", "En riesgo"),
    (0.0, "CRITICAL", "Crítico"),
)


def trapezoid(x: float, a: float, b: float, c: float, d: float) -> float:
    if x < a or x > d:
        return 0.0
    if b <= x <= c:
        return 1.0
    if x < b:
        return (x - a) / (b - a) if b > a else 1.0
    return (d - x) / (d - c) if d > c else 1.0


def memberships(deviation: float) -> dict[str, float]:
    d = max(0.0, deviation)
    return {
        "ideal": trapezoid(d, -1.0, 0.0, 0.0, 0.25),
        "acceptable": trapezoid(d, 0.0, 0.25, 0.4, 0.7),
        "deviated": trapezoid(d, 0.4, 0.7, 0.9, 1.2),
        "critical": trapezoid(d, 0.9, 1.2, 1e9, 1e9),
    }


def parameter_health(deviation: float) -> float:
    degrees = memberships(deviation)
    total = sum(degrees.values())
    if total == 0:
        return OUTPUTS["critical"]
    return sum(OUTPUTS[term] * degree for term, degree in degrees.items()) / total


@dataclass(frozen=True)
class HealthResult:
    index: float
    level: str
    label: str
    by_parameter: dict[str, float]


def health_index(deviations: dict[str, float]) -> HealthResult:
    if not deviations:
        return HealthResult(0.0, "UNKNOWN", "Sin datos", {})
    scores = {parameter: parameter_health(dev) for parameter, dev in deviations.items()}
    weighted = sum(scores[p] * WEIGHTS.get(p, 1.0) for p in scores) / sum(WEIGHTS.get(p, 1.0) for p in scores)
    worst_critical = max(memberships(dev)["critical"] for dev in deviations.values())
    # Regla de peor caso: una variable plenamente crítica deja la salud en «En riesgo» como máximo.
    cap = 100.0 - 60.0 * worst_critical
    index = round(min(weighted, cap), 1)
    level, label = next((lvl, lbl) for threshold, lvl, lbl in LEVELS if index >= threshold)
    return HealthResult(index, level, label, {p: round(s, 1) for p, s in scores.items()})

"""Pronóstico de tendencia por variable a partir del historial con su hora de medición.

Se ajusta una recta con el estimador de Theil-Sen (mediana de las pendientes entre pares de
lecturas), que tolera lecturas atípicas mejor que los mínimos cuadrados. Con la pendiente por hora
se estima el valor dentro de 3 horas y, si la tendencia empuja la variable fuera de su rango
ideal, en cuántas horas cruzará el límite. La luz no se pronostica: sigue el ciclo del día.
"""

from dataclasses import dataclass
from datetime import datetime

import numpy as np

from app.knowledge.profiles import CropProfile

FORECAST_PARAMETERS = ("temperature", "humidity", "ph", "tds", "soilMoisture")
MIN_POINTS = 6
MIN_SPAN_HOURS = 0.3
HORIZON_HOURS = 3.0
MAX_HOURS_TO_LIMIT = 24.0
# Una variable es estable si en 3 h no se mueve más del 10 % del ancho de su rango ideal.
STABLE_SHARE = 0.10


@dataclass(frozen=True)
class Forecast:
    parameter: str
    current: float
    slope_per_hour: float
    expected_in_3h: float
    trend: str
    hours_to_limit: float | None
    limit: str | None
    confidence: float
    message: str


def theil_sen(hours: np.ndarray, values: np.ndarray) -> tuple[float, float, float]:
    """Pendiente, intercepto y concordancia (fracción de pendientes con el signo de la mediana)."""
    i, j = np.triu_indices(len(hours), k=1)
    dt = hours[j] - hours[i]
    valid = dt > 1e-9
    slopes = (values[j] - values[i])[valid] / dt[valid]
    if slopes.size == 0:
        return 0.0, float(np.median(values)), 0.0
    slope = float(np.median(slopes))
    intercept = float(np.median(values - slope * hours))
    agreement = float(np.mean(np.sign(slopes) == np.sign(slope))) if slope != 0 else 1.0
    return slope, intercept, agreement


def forecast(profile: CropProfile, history: list[tuple[datetime, dict[str, float | None]]]) -> list[Forecast]:
    points = sorted(((moment, measures) for moment, measures in history if moment is not None), key=lambda p: p[0])
    if len(points) < MIN_POINTS:
        return []
    origin = points[-1][0]
    results = []
    for parameter in FORECAST_PARAMETERS:
        series = [((moment - origin).total_seconds() / 3600, measures.get(parameter))
                  for moment, measures in points if measures.get(parameter) is not None]
        if len(series) < MIN_POINTS:
            continue
        hours = np.array([h for h, _ in series], dtype=float)
        values = np.array([v for _, v in series], dtype=float)
        if hours.max() - hours.min() < MIN_SPAN_HOURS:
            continue
        slope, intercept, agreement = theil_sen(hours, values)
        results.append(_describe(profile, parameter, float(values[-1]), slope, intercept, agreement))
    return results


def _describe(profile: CropProfile, parameter: str, current: float, slope: float, intercept: float,
              agreement: float) -> Forecast:
    rng = profile.ranges[parameter]
    width = rng.max - rng.min
    expected = intercept + slope * HORIZON_HOURS
    trend = "STABLE"
    if abs(slope) * HORIZON_HOURS >= STABLE_SHARE * width:
        trend = "RISING" if slope > 0 else "FALLING"

    # Las horas al límite parten del nivel ajustado (no de la última lectura, que trae ruido),
    # igual que el valor esperado en 3 h: así ambos datos son coherentes entre sí.
    level = intercept
    hours_to_limit, limit = None, None
    if trend != "STABLE" and rng.min <= level <= rng.max:
        hours = (rng.max - level) / slope if slope > 0 else (level - rng.min) / -slope
        if hours <= MAX_HOURS_TO_LIMIT:
            hours_to_limit, limit = round(hours, 2), "MAX" if slope > 0 else "MIN"

    label = rng.label[:1].upper() + rng.label[1:]
    per_hour = f"{_fmt(abs(slope))} {rng.unit} por hora"
    if trend == "STABLE":
        message = f"{label} se mantiene estable."
    elif hours_to_limit is not None:
        bound = rng.max if limit == "MAX" else rng.min
        verb = "sube" if slope > 0 else "baja"
        message = (f"{label} {verb} {per_hour}: llegará al {'máximo' if limit == 'MAX' else 'mínimo'} "
                   f"({_fmt(bound)} {rng.unit}) en {_hours(hours_to_limit)}.")
    else:
        verb = "sube" if slope > 0 else "baja"
        message = f"{label} {verb} {per_hour}."

    return Forecast(parameter, round(current, 2), round(slope, 3), round(expected, 2), trend, hours_to_limit,
                    limit, round(agreement, 2), message)


def _fmt(value: float) -> str:
    return f"{value:.1f}".rstrip("0").rstrip(".")


def _hours(value: float) -> str:
    if value < 1:
        return f"unos {max(5, round(value * 60 / 5) * 5)} minutos"
    return f"unas {round(value)} h" if value >= 1.5 else "alrededor de 1 h"

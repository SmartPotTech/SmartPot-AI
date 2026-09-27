"""Calidad de los datos antes de entrenar.

- Completitud: fracción de valores presentes.
- Validez: fracción de valores dentro de la escala física de los sensores.
- Atípicos por rango intercuartílico (IQR con k = 3): se excluyen del entrenamiento para que un sensor
  dañado no enseñe patrones falsos.

El puntaje de calidad (DQS) es el producto de la completitud, la validez y la fracción de filas útiles.
"""

from dataclasses import dataclass

import numpy as np

from app.learning.store import COLUMNS

PHYSICAL = {
    "temperature": (-20, 60), "humidity": (0, 100), "brightness": (0, 200_000), "ph": (0, 14),
    "tds": (0, 10_000), "atmosphere": (300, 1_100), "soil_moisture": (0, 100),
}
IQR_K = 3.0


@dataclass(frozen=True)
class QualityReport:
    rows: int
    completeness: float
    validity: float
    outliers: int
    score: float
    keep: np.ndarray


def assess(values: np.ndarray) -> QualityReport:
    rows = values.shape[0]
    if rows == 0:
        return QualityReport(0, 0.0, 0.0, 0, 0.0, np.zeros(0, dtype=bool))
    present = ~np.isnan(values)
    completeness = float(present.mean())
    valid = np.ones_like(present)
    for i, name in enumerate(COLUMNS):
        low, high = PHYSICAL[name]
        with np.errstate(invalid="ignore"):
            valid[:, i] = ~present[:, i] | ((values[:, i] >= low) & (values[:, i] <= high))
    validity = float(valid[present].mean()) if present.any() else 0.0

    outlier = np.zeros(rows, dtype=bool)
    for i in range(values.shape[1]):
        data = values[present[:, i], i]
        if data.size < 20:
            continue
        q1, q3 = np.percentile(data, [25, 75])
        # Una variable casi constante no convierte en atípico cualquier cambio pequeño.
        low, high = PHYSICAL[COLUMNS[i]]
        spread = max(q3 - q1, 0.02 * (high - low))
        with np.errstate(invalid="ignore"):
            outlier |= present[:, i] & ((values[:, i] < q1 - IQR_K * spread) | (values[:, i] > q3 + IQR_K * spread))
    keep = ~outlier & valid.all(axis=1)
    score = completeness * validity * float(keep.mean())
    return QualityReport(rows, round(completeness, 4), round(validity, 4), int(outlier.sum()), round(score, 4), keep)

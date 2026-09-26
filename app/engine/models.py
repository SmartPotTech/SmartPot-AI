"""Modelos de aprendizaje automático entrenados al arrancar con datos sintéticos.

Las variables se normalizan respecto al perfil de cada especie (0 = mínimo ideal, 1 = máximo
ideal), así un mismo modelo sirve para todos los cultivos:

- Regresión logística: necesidad de ventilación a partir de temperatura y humedad (frontera lineal).
- Red neuronal (MLP): necesidad de corregir el pH a partir de pH y nutrientes. La etiqueta es
  «fuera de rango por cualquiera de los dos lados», una frontera que no es lineal.
- Isolation Forest: qué tan improbable es físicamente la lectura (posible sensor dañado), combinado con
  saltos bruscos frente al historial.
"""

import logging
import math
from dataclasses import dataclass, field

import numpy as np
from sklearn.ensemble import IsolationForest
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import train_test_split
from sklearn.neural_network import MLPClassifier
from sklearn.pipeline import make_pipeline
from sklearn.preprocessing import StandardScaler

from app.knowledge.profiles import PARAMETERS, CropProfile

log = logging.getLogger(__name__)

SAMPLES = 4000
LABEL_NOISE = 0.04
SPIKE_Z = 4.0


def normalize(profile: CropProfile, measures: dict[str, float | None]) -> dict[str, float]:
    """Posición de cada valor dentro de su rango ideal (0 a 1 dentro del rango)."""
    positions = {}
    for parameter in PARAMETERS:
        value = measures.get(parameter)
        if value is None:
            continue
        rng = profile.ranges[parameter]
        positions[parameter] = (value - rng.min) / (rng.max - rng.min)
    return positions


@dataclass
class ModelRegistry:
    seed: int = 42
    metrics: dict[str, float] = field(default_factory=dict)

    def __post_init__(self) -> None:
        rng = np.random.default_rng(self.seed)
        self._train_ventilation(rng)
        self._train_ph_correction(rng)
        self._train_anomaly(rng)
        log.info("Modelos entrenados: %s", {k: round(v, 3) for k, v in self.metrics.items()})

    @staticmethod
    def _positions(rng: np.random.Generator, size: int) -> np.ndarray:
        inside = rng.uniform(-0.3, 1.3, size=(int(size * 0.7), 2))
        wide = rng.uniform(-1.0, 2.0, size=(size - inside.shape[0], 2))
        return np.vstack([inside, wide])

    @staticmethod
    def _noisy(labels: np.ndarray, rng: np.random.Generator) -> np.ndarray:
        flip = rng.random(labels.shape[0]) < LABEL_NOISE
        return np.where(flip, 1 - labels, labels)

    @staticmethod
    def _ventilation_features(positions: np.ndarray) -> np.ndarray:
        # El exceso sobre el máximo ideal vuelve lineal la regla «calor o humedad altos».
        excess = np.clip(positions - 1.0, 0.0, None)
        return np.hstack([positions, excess])

    def _train_ventilation(self, rng: np.random.Generator) -> None:
        positions = self._positions(rng, SAMPLES)
        y = self._noisy(((positions[:, 0] > 1.0) | (positions[:, 1] > 1.0)).astype(int), rng)
        x = self._ventilation_features(positions)
        x_train, x_test, y_train, y_test = train_test_split(x, y, test_size=0.2, random_state=self.seed)
        self.ventilation = make_pipeline(StandardScaler(), LogisticRegression())
        self.ventilation.fit(x_train, y_train)
        self.metrics["ventilation_accuracy"] = float(self.ventilation.score(x_test, y_test))

    def _train_ph_correction(self, rng: np.random.Generator) -> None:
        x = self._positions(rng, SAMPLES)
        y = self._noisy(((x[:, 0] < 0.0) | (x[:, 0] > 1.0)).astype(int), rng)
        x_train, x_test, y_train, y_test = train_test_split(x, y, test_size=0.2, random_state=self.seed)
        self.ph_correction = make_pipeline(
            StandardScaler(),
            MLPClassifier(hidden_layer_sizes=(8, 8), max_iter=2000, random_state=self.seed),
        )
        self.ph_correction.fit(x_train, y_train)
        self.metrics["ph_correction_accuracy"] = float(self.ph_correction.score(x_test, y_test))

    def _train_anomaly(self, rng: np.random.Generator) -> None:
        # Lecturas plausibles, aunque estén fuera del rango ideal: solo lo físicamente raro es atípico.
        plausible = rng.uniform(-1.5, 2.5, size=(SAMPLES, len(PARAMETERS)))
        typical = rng.normal(loc=0.5, scale=0.5, size=(SAMPLES, len(PARAMETERS)))
        normal = np.vstack([plausible, typical])
        self.anomaly = IsolationForest(n_estimators=150, contamination=0.02, random_state=self.seed)
        self.anomaly.fit(normal)
        extreme = rng.uniform(3.5, 7.0, size=(200, len(PARAMETERS)))
        detected = (self.anomaly.predict(extreme) == -1).mean()
        self.metrics["anomaly_detection_rate"] = float(detected)

    def ventilation_probability(self, positions: dict[str, float]) -> float | None:
        if "temperature" not in positions or "humidity" not in positions:
            return None
        x = self._ventilation_features(np.array([[positions["temperature"], positions["humidity"]]]))
        return float(self.ventilation.predict_proba(x)[0, 1])

    def ph_correction_probability(self, positions: dict[str, float]) -> float | None:
        if "ph" not in positions:
            return None
        x = np.array([[positions["ph"], positions.get("tds", 0.5)]])
        return float(self.ph_correction.predict_proba(x)[0, 1])

    def anomaly_probability(self, positions: dict[str, float], history: list[dict[str, float]]) -> float:
        vector = np.array([[positions.get(p, 0.5) for p in PARAMETERS]])
        score = float(self.anomaly.decision_function(vector)[0])
        forest = 1.0 / (1.0 + math.exp(25.0 * score))
        return round(max(forest, self._spike_probability(positions, history)), 3)

    @staticmethod
    def _spike_probability(positions: dict[str, float], history: list[dict[str, float]]) -> float:
        if len(history) < 10:
            return 0.0
        worst = 0.0
        for parameter, value in positions.items():
            series = np.array([h[parameter] for h in history if parameter in h])
            if series.size < 10:
                continue
            spread = max(float(series.std()), 0.05)
            worst = max(worst, abs(value - float(series.mean())) / spread)
        return float(min(1.0, max(0.0, (worst - SPIKE_Z) / SPIKE_Z + 0.5))) if worst > SPIKE_Z else 0.0

"""Aprendizaje continuo: guarda las lecturas que envía la API, reentrena cuando llegan suficientes datos
nuevos y entrega lo aprendido en cada evaluación.

El entrenamiento corre en un proceso aparte para no frenar las evaluaciones en curso. Los modelos de
cada especie se guardan con joblib en el volumen del servicio y se cargan al arrancar.
"""

import logging
import threading
import time
from concurrent.futures import ProcessPoolExecutor
from dataclasses import dataclass, field
from multiprocessing import get_context
from pathlib import Path
from typing import Any

import joblib
import numpy as np
import sklearn

from app.knowledge.profiles import PARAMETERS, PROFILES, get_profile
from app.learning.features import build, live_features, to_store
from app.learning.quality import assess
from app.learning.store import ReadingStore, Series, StoredReading
from app.learning.trainer import AnomalyModel, ModelCard, StatesModel, train_all

log = logging.getLogger(__name__)

COUNTS_TTL = 30.0
PRUNE_EVERY = 6 * 3600.0
LEARNED_TASKS = ("needs_water", "overheat")


@dataclass
class TypeState:
    cards: dict[str, ModelCard] = field(default_factory=dict)
    states: StatesModel | None = None
    anomaly: AnomalyModel | None = None
    quality: dict[str, float] = field(default_factory=dict)
    trained_at: float | None = None
    readings_at_training: int = 0

    @property
    def usable(self) -> bool:
        return any(card.usable for card in self.cards.values()) or self.states is not None


@dataclass(frozen=True)
class LearningConfig:
    data_dir: str | None
    min_samples: int = 200
    retrain_every: int = 300
    check_seconds: int = 300
    retention_days: int = 60
    max_rows: int = 200_000
    training_rows: int = 12_000
    separate_process: bool = True


def fit_type(crop_type: str, series: Series, min_samples: int, current: TypeState | None) -> TypeState:
    """Entrenamiento completo de una especie. Es una función de módulo para poder correr en otro proceso."""
    profile = get_profile(crop_type)
    report = assess(series.values)
    clean = series.subset(report.keep)
    dataset = build(profile, clean)
    models = train_all(dataset, min_samples, dict(current.cards) if current else {})
    return TypeState(
        cards={task: models[task] for task in ("needs_water", "overheat", "moisture_1h")},
        states=models["states"] or (current.states if current else None),
        anomaly=models["anomaly"] or (current.anomaly if current else None),
        quality={"rows": report.rows, "completeness": report.completeness, "validity": report.validity,
                 "outliers": report.outliers, "score": report.score},
        trained_at=time.time(),
        readings_at_training=len(series),
    )


class LearningService:
    def __init__(self, config: LearningConfig):
        self.config = config
        self.store = ReadingStore(config.data_dir)
        self.models_dir = Path(config.data_dir) / "models" if self.store.persistent else None
        self.types: dict[str, TypeState] = {}
        self.training: set[str] = set()
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._executor: ProcessPoolExecutor | None = None
        self._counts: tuple[float, dict] = (0.0, {})
        self._pruned_at = 0.0
        self._load()

    # --- ciclo de vida -------------------------------------------------------------------------------------

    def start(self) -> None:
        if self.config.check_seconds <= 0 or self._thread is not None:
            return
        self._thread = threading.Thread(target=self._loop, name="aprendizaje", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=5)
        if self._executor is not None:
            self._executor.shutdown(wait=False, cancel_futures=True)
        self.store.close()

    def _loop(self) -> None:
        while not self._stop.wait(self.config.check_seconds):
            try:
                if time.time() - self._pruned_at > PRUNE_EVERY:
                    removed = self.store.prune(self.config.retention_days, self.config.max_rows)
                    self._pruned_at = time.time()
                    if removed:
                        log.info("Aprendizaje: %d lecturas antiguas eliminadas", removed)
                for crop_type in self.due():
                    self.train(crop_type)
            except Exception:  # noqa: BLE001 - el ciclo en segundo plano nunca debe morir
                log.exception("Falló el ciclo de aprendizaje")

    # --- datos ---------------------------------------------------------------------------------------------

    def ingest(self, readings: list[StoredReading]) -> int:
        stored = self.store.add(readings)
        self._counts = (0.0, {})
        return stored

    def forget(self, crop_id: str) -> int:
        removed = self.store.forget(crop_id)
        self._counts = (0.0, {})
        return removed

    def counts(self) -> dict[str, dict]:
        cached_at, counts = self._counts
        if time.time() - cached_at > COUNTS_TTL:
            counts = self.store.counts()
            self._counts = (time.time(), counts)
        return counts

    def due(self) -> list[str]:
        due = []
        for crop_type, info in self.counts().items():
            state = self.types.get(crop_type)
            done = state.readings_at_training if state else 0
            fresh = info["readings"] - done
            if crop_type in self.training:
                continue
            if (state is None and info["readings"] >= self.config.min_samples) or \
                    (state is not None and fresh >= self.config.retrain_every):
                due.append(crop_type)
        return due

    # --- entrenamiento -------------------------------------------------------------------------------------

    def train(self, crop_type: str) -> TypeState | None:
        with self._lock:
            if crop_type in self.training:
                return self.types.get(crop_type)
            self.training.add(crop_type)
        try:
            series = self.store.load(crop_type, self.config.training_rows)
            if len(series) == 0:
                return None
            started = time.time()
            state = self._run(crop_type, series, self.types.get(crop_type))
            state.readings_at_training = self.counts().get(crop_type, {}).get("readings", len(series))
            with self._lock:
                self.types[crop_type] = state
            self._save(crop_type, state)
            log.info("Aprendizaje de %s: %d lecturas en %.1f s (%s)", crop_type, len(series), time.time() - started,
                     ", ".join(f"{t}={c.status}" for t, c in state.cards.items()))
            return state
        finally:
            with self._lock:
                self.training.discard(crop_type)

    def _run(self, crop_type: str, series: Series, current: TypeState | None) -> TypeState:
        if not self.config.separate_process:
            return fit_type(crop_type, series, self.config.min_samples, current)
        try:
            if self._executor is None:
                self._executor = ProcessPoolExecutor(max_workers=1, mp_context=get_context("spawn"))
            return self._executor.submit(fit_type, crop_type, series, self.config.min_samples, current).result()
        except (OSError, RuntimeError) as error:
            log.warning("Entrenamiento en el mismo proceso (%s)", error)
            self._executor = None
            return fit_type(crop_type, series, self.config.min_samples, current)

    # --- persistencia --------------------------------------------------------------------------------------

    def _save(self, crop_type: str, state: TypeState) -> None:
        if self.models_dir is None:
            return
        self.models_dir.mkdir(parents=True, exist_ok=True)
        target = self.models_dir / f"{crop_type}.joblib"
        temporary = target.with_suffix(".tmp")
        joblib.dump({"sklearn": sklearn.__version__, "state": state}, temporary)
        temporary.replace(target)

    def _load(self) -> None:
        if self.models_dir is None or not self.models_dir.exists():
            return
        for path in self.models_dir.glob("*.joblib"):
            try:
                saved = joblib.load(path)
            except Exception as error:  # noqa: BLE001 - un archivo dañado no impide arrancar
                log.warning("No se pudo cargar %s: %s", path.name, error)
                continue
            if saved.get("sklearn") != sklearn.__version__:
                log.info("Modelos de %s entrenados con otra versión de scikit-learn: se reentrenan", path.stem)
                continue
            self.types[path.stem] = saved["state"]
        if self.types:
            log.info("Modelos aprendidos cargados: %s", ", ".join(sorted(self.types)))

    # --- consulta ------------------------------------------------------------------------------------------

    def infer(self, crop_type: str, measures: dict[str, float | None], hour: int | None, moment: float,
              history: list[tuple[float, dict[str, float | None]]]) -> dict[str, Any]:
        profile = get_profile(crop_type)
        readings = int(self.counts().get(crop_type, {}).get("readings", 0))
        state = self.types.get(crop_type)
        name = profile.name.lower()
        if state is None or not state.usable:
            return {"source": "BASE", "readings": readings, "trained_at": None,
                    "message": f"Aún aprendo de las macetas de {name}: hay {readings} lecturas reales y el primer "
                               f"entrenamiento empieza con {self.config.min_samples}."}

        result: dict[str, Any] = {"source": "LEARNED", "readings": readings, "trained_at": state.trained_at,
                                  "predictions": [], "moisture": None, "state": None, "anomaly": None}
        features = live_features(profile, measures, hour, moment, history)
        if features is not None:
            soil_pos = features[0, PARAMETERS.index("soilMoisture")]
            temp_pos = features[0, PARAMETERS.index("temperature")]
            applicable = {"needs_water": soil_pos >= 0, "overheat": temp_pos <= 1}
            for task in LEARNED_TASKS:
                card = state.cards.get(task)
                if card and card.usable and applicable[task]:
                    probability = float(card.estimator.predict_proba(features)[0, 1])
                    result["predictions"].append({"name": task, "label": card.label,
                                                  "probability": round(probability, 3), "model": card.model,
                                                  "metric": card.metric, "score": card.holdout})
            card = state.cards.get("moisture_1h")
            current = measures.get("soilMoisture")
            if card and card.usable and current is not None:
                delta = float(card.estimator.predict(features)[0])
                result["moisture"] = {"expected": round(min(100.0, max(0.0, current + delta)), 1),
                                      "model": card.model, "mae": card.holdout}
        if features is not None:
            pos = features[:, :len(PARAMETERS)]
            if state.states is not None:
                cluster = state.states.assign(pos)
                result["state"] = {"label": cluster.label, "description": cluster.description,
                                   "share": cluster.share}
            if state.anomaly is not None:
                score, unusual = state.anomaly.score(pos)
                result["anomaly"] = {"score": score, "unusual": unusual}
        trained = sum(1 for card in state.cards.values() if card.usable)
        result["message"] = (f"Aprendido de {readings:,} lecturas reales de {name}".replace(",", ".")
                             + f": {trained} de 3 modelos supervisados entrenados.")
        return result

    def status(self) -> dict[str, Any]:
        counts = self.counts()
        crop_types = []
        for crop_type in sorted(set(counts) | set(self.types)):
            info = counts.get(crop_type, {"readings": 0, "crops": 0})
            state = self.types.get(crop_type)
            crop_types.append({
                "crop_type": crop_type,
                "name": PROFILES[crop_type].name if crop_type in PROFILES else crop_type,
                "readings": int(info["readings"]),
                "crops": int(info["crops"]),
                "new_since_training": int(info["readings"]) - (state.readings_at_training if state else 0),
                "training": crop_type in self.training,
                "trained_at": state.trained_at if state else None,
                "quality": state.quality if state else None,
                "models": [_card(card) for card in state.cards.values()] if state else [],
                "states": _states(state.states) if state and state.states else None,
                "anomaly": {"samples": state.anomaly.samples, "contamination": 0.01}
                if state and state.anomaly else None,
            })
        return {"enabled": True, "persistent": self.store.persistent, "stored_readings": self.store.total(),
                "min_samples": self.config.min_samples, "retrain_every": self.config.retrain_every,
                "crop_types": crop_types}


def _plain(value: Any) -> Any:
    return value.item() if isinstance(value, np.generic) else value


def _card(card: ModelCard) -> dict[str, Any]:
    return {
        "task": card.task, "label": card.label, "status": card.status, "reason": card.reason,
        "metric": card.metric, "model": card.model, "score": card.score, "std": card.std, "holdout": card.holdout,
        "baseline": card.baseline, "samples": card.samples, "positives": card.positives, "version": card.version,
        "trained_at": card.trained_at, "params": {k: _plain(v) for k, v in card.params.items()},
        "candidates": [{"model": c.model, "score": _nan_to_none(c.score), "std": _nan_to_none(c.std)}
                       for c in card.candidates],
    }


def _states(states: StatesModel) -> dict[str, Any]:
    return {"k": states.k, "silhouette": states.silhouette,
            "clusters": [{"label": c.label, "description": c.description, "share": c.share}
                         for c in states.clusters]}


def _nan_to_none(value: float | None) -> float | None:
    return None if value is None or np.isnan(value) else value


def to_stored(crop_id: str, crop_type: str, measured_at: float, local_hour: int | None,
              measures: dict[str, float | None]) -> StoredReading:
    return StoredReading(crop_id=crop_id, crop_type=crop_type, measured_at=measured_at, local_hour=local_hour,
                         values=to_store(measures))

"""Almacén de lecturas reales para el aprendizaje (SQLite en el volumen del servicio).

El id del cultivo se guarda seudonimizado (hash SHA-256): el servicio aprende de la serie de cada
maceta sin conocer a qué cultivo ni a qué persona pertenece. Si la carpeta de datos no se puede
escribir, el almacén trabaja en memoria y el aprendizaje se pierde al reiniciar.
"""

import hashlib
import logging
import sqlite3
import threading
import time
from dataclasses import dataclass
from pathlib import Path

import numpy as np

log = logging.getLogger(__name__)

COLUMNS = ("temperature", "humidity", "brightness", "ph", "tds", "atmosphere", "soil_moisture")
SCHEMA = f"""
CREATE TABLE IF NOT EXISTS readings (
    crop_key TEXT NOT NULL,
    crop_type TEXT NOT NULL,
    measured_at REAL NOT NULL,
    local_hour INTEGER,
    {", ".join(f"{column} REAL" for column in COLUMNS)},
    received_at REAL NOT NULL,
    PRIMARY KEY (crop_key, measured_at)
);
CREATE INDEX IF NOT EXISTS readings_type_time ON readings (crop_type, measured_at);
"""


def crop_key(crop_id: str) -> str:
    return hashlib.sha256(crop_id.encode("utf-8")).hexdigest()[:24]


@dataclass(frozen=True)
class StoredReading:
    crop_id: str
    crop_type: str
    measured_at: float
    local_hour: int | None
    values: dict[str, float | None]


@dataclass
class Series:
    """Lecturas de una especie ordenadas por maceta y hora; los valores ausentes son NaN."""

    crops: np.ndarray
    times: np.ndarray
    hours: np.ndarray
    values: np.ndarray

    def __len__(self) -> int:
        return int(self.times.shape[0])

    def subset(self, mask: np.ndarray) -> "Series":
        return Series(self.crops[mask], self.times[mask], self.hours[mask], self.values[mask])


class ReadingStore:
    def __init__(self, data_dir: str | Path | None):
        self.path = self._resolve(data_dir)
        self._lock = threading.Lock()
        self._connection = sqlite3.connect(self.path or ":memory:", check_same_thread=False)
        self._connection.executescript(SCHEMA)
        self._connection.commit()

    @property
    def persistent(self) -> bool:
        return self.path is not None

    @staticmethod
    def _resolve(data_dir: str | Path | None) -> str | None:
        if not data_dir:
            return None
        folder = Path(data_dir)
        try:
            folder.mkdir(parents=True, exist_ok=True)
            probe = folder / ".write-test"
            probe.write_text("ok", encoding="utf-8")
            probe.unlink()
            return str(folder / "learning.db")
        except OSError as error:
            log.warning("No se puede escribir en %s (%s): el aprendizaje queda en memoria", folder, error)
            return None

    def add(self, readings: list[StoredReading]) -> int:
        """Guarda las lecturas nuevas; las repetidas (misma maceta y hora) se ignoran."""
        now = time.time()
        rows = [(crop_key(r.crop_id), r.crop_type, r.measured_at, r.local_hour,
                 *(r.values.get(column) for column in COLUMNS), now) for r in readings]
        placeholders = ", ".join("?" for _ in range(len(COLUMNS) + 5))
        with self._lock:
            before = self._connection.total_changes
            self._connection.executemany(f"INSERT OR IGNORE INTO readings VALUES ({placeholders})", rows)
            self._connection.commit()
            return self._connection.total_changes - before

    def counts(self) -> dict[str, dict[str, float]]:
        with self._lock:
            rows = self._connection.execute(
                "SELECT crop_type, COUNT(*), COUNT(DISTINCT crop_key), MAX(received_at) FROM readings "
                "GROUP BY crop_type").fetchall()
        return {row[0]: {"readings": row[1], "crops": row[2], "last_received": row[3]} for row in rows}

    def total(self) -> int:
        with self._lock:
            return int(self._connection.execute("SELECT COUNT(*) FROM readings").fetchone()[0])

    def load(self, crop_type: str, limit: int) -> Series:
        """Las `limit` lecturas más recientes de la especie, ordenadas por maceta y hora."""
        columns = ", ".join(COLUMNS)
        with self._lock:
            rows = self._connection.execute(
                f"SELECT crop_key, measured_at, local_hour, {columns} FROM "
                f"(SELECT * FROM readings WHERE crop_type = ? ORDER BY measured_at DESC LIMIT ?) "
                f"ORDER BY crop_key, measured_at", (crop_type, limit)).fetchall()
        if not rows:
            empty = np.empty(0)
            return Series(np.empty(0, dtype=object), empty, empty, np.empty((0, len(COLUMNS))))
        crops = np.array([row[0] for row in rows], dtype=object)
        times = np.array([row[1] for row in rows], dtype=float)
        hours = np.array([np.nan if row[2] is None else row[2] for row in rows], dtype=float)
        values = np.array([[np.nan if v is None else v for v in row[3:]] for row in rows], dtype=float)
        return Series(crops, times, hours, values)

    def forget(self, crop_id: str) -> int:
        with self._lock:
            cursor = self._connection.execute("DELETE FROM readings WHERE crop_key = ?", (crop_key(crop_id),))
            self._connection.commit()
            return cursor.rowcount

    def prune(self, retention_days: int, max_rows_per_type: int) -> int:
        """Borra lo más antiguo: por antigüedad y por tope de lecturas por especie."""
        cutoff = time.time() - retention_days * 86_400
        with self._lock:
            removed = self._connection.execute("DELETE FROM readings WHERE measured_at < ?", (cutoff,)).rowcount
            for (crop_type,) in self._connection.execute("SELECT DISTINCT crop_type FROM readings").fetchall():
                removed += self._connection.execute(
                    "DELETE FROM readings WHERE crop_type = ? AND measured_at < ("
                    "SELECT measured_at FROM readings WHERE crop_type = ? ORDER BY measured_at DESC "
                    "LIMIT 1 OFFSET ?)", (crop_type, crop_type, max(0, max_rows_per_type - 1))).rowcount
            self._connection.commit()
        return removed

    def close(self) -> None:
        with self._lock:
            self._connection.close()

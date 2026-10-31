"""Puente nativo C++ para aceleración numérica directa de SmartPot-AI.

Carga la biblioteca compilada (libsmartpot_core.so en Linux o smartpot_core.dll en Windows)
y expone llamadas C ABI con tipado estricto ctypes. Si la biblioteca no está compilada,
ofrece detección transparente para fallback seguro en Python.
"""

from __future__ import annotations

import ctypes
import logging
import os
import sys
from pathlib import Path
from typing import Any

import numpy as np

log = logging.getLogger(__name__)


# --- Estructuras ctypes alineadas a 8 bytes (#pragma pack(push, 8)) ---


class CTheilSenResult(ctypes.Structure):
    _pack_ = 8
    _fields_ = [
        ("slope", ctypes.c_double),
        ("intercept", ctypes.c_double),
        ("agreement", ctypes.c_double),
    ]


class CHealthResult(ctypes.Structure):
    _pack_ = 8
    _fields_ = [
        ("index", ctypes.c_double),
        ("level", ctypes.c_char * 16),
        ("label", ctypes.c_char * 32),
        ("param_count", ctypes.c_int32),
        ("param_names", (ctypes.c_char * 32) * 8),
        ("param_scores", ctypes.c_double * 8),
    ]


class CParamRange(ctypes.Structure):
    _pack_ = 8
    _fields_ = [
        ("parameter", ctypes.c_char * 32),
        ("min_val", ctypes.c_double),
        ("max_val", ctypes.c_double),
        ("unit", ctypes.c_char * 16),
        ("label", ctypes.c_char * 32),
    ]


class CForecastResult(ctypes.Structure):
    _pack_ = 8
    _fields_ = [
        ("parameter", ctypes.c_char * 32),
        ("current", ctypes.c_double),
        ("slope_per_hour", ctypes.c_double),
        ("expected_in_3h", ctypes.c_double),
        ("trend", ctypes.c_char * 16),
        ("has_hours_to_limit", ctypes.c_int32),
        ("hours_to_limit", ctypes.c_double),
        ("limit", ctypes.c_char * 8),
        ("confidence", ctypes.c_double),
        ("message", ctypes.c_char * 256),
    ]


class CPhysicalBound(ctypes.Structure):
    _pack_ = 8
    _fields_ = [
        ("min_val", ctypes.c_double),
        ("max_val", ctypes.c_double),
    ]


class CQualityReport(ctypes.Structure):
    _pack_ = 8
    _fields_ = [
        ("rows", ctypes.c_int32),
        ("cols", ctypes.c_int32),
        ("completeness", ctypes.c_double),
        ("validity", ctypes.c_double),
        ("outliers", ctypes.c_double),
        ("score", ctypes.c_double),
        ("keep_count", ctypes.c_int32),
    ]


class NativeEngine:
    def __init__(self) -> None:
        self.lib: ctypes.CDLL | None = None
        self._load_library()
        if self.lib:
            self._setup_bindings()

    def _load_library(self) -> None:
        candidates: list[Path] = []
        base_dir = Path(__file__).resolve().parent.parent / "native"

        if sys.platform == "win32":
            candidates.append(base_dir / "smartpot_core.dll")
            candidates.append(Path("smartpot_core.dll"))
        elif sys.platform == "darwin":
            candidates.append(base_dir / "libsmartpot_core.dylib")
        else:
            candidates.append(base_dir / "libsmartpot_core.so")
            candidates.append(Path("/app/app/native/libsmartpot_core.so"))
            candidates.append(Path("/usr/local/lib/libsmartpot_core.so"))

        # Variable de entorno de anulación explícita
        custom_path = os.environ.get("SMARTPOT_CORE_LIB")
        if custom_path:
            candidates.insert(0, Path(custom_path))

        for path in candidates:
            if path.exists():
                try:
                    self.lib = ctypes.CDLL(str(path))
                    log.info("Motor nativo C++ cargado exitosamente desde: %s", path)
                    return
                except Exception as e:
                    log.warning("No se pudo cargar la biblioteca nativa en %s: %s", path, e)

        log.info("Motor nativo C++ no disponible; operando con motor estándar en Python")

    def _setup_bindings(self) -> None:
        assert self.lib is not None

        # smartpot_theil_sen
        self.lib.smartpot_theil_sen.argtypes = [
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
            ctypes.c_int,
            ctypes.POINTER(CTheilSenResult),
        ]
        self.lib.smartpot_theil_sen.restype = ctypes.c_int

        # smartpot_fuzzy_health_index
        self.lib.smartpot_fuzzy_health_index.argtypes = [
            ctypes.POINTER(ctypes.c_char_p),
            ctypes.POINTER(ctypes.c_double),
            ctypes.c_int,
            ctypes.POINTER(CHealthResult),
        ]
        self.lib.smartpot_fuzzy_health_index.restype = ctypes.c_int

        # smartpot_forecast_describe
        self.lib.smartpot_forecast_describe.argtypes = [
            ctypes.POINTER(CParamRange),
            ctypes.c_double,
            ctypes.c_double,
            ctypes.c_double,
            ctypes.c_double,
            ctypes.POINTER(CForecastResult),
        ]
        self.lib.smartpot_forecast_describe.restype = ctypes.c_int

        # smartpot_quality_assess
        self.lib.smartpot_quality_assess.argtypes = [
            ctypes.POINTER(ctypes.c_double),
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(CPhysicalBound),
            ctypes.POINTER(CQualityReport),
            ctypes.POINTER(ctypes.c_uint8),
        ]
        self.lib.smartpot_quality_assess.restype = ctypes.c_int

        # smartpot_features_normalize
        self.lib.smartpot_features_normalize.argtypes = [
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(CParamRange),
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
        ]
        self.lib.smartpot_features_normalize.restype = ctypes.c_int

        # smartpot_features_circular_hour
        self.lib.smartpot_features_circular_hour.argtypes = [
            ctypes.c_double,
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
        ]
        self.lib.smartpot_features_circular_hour.restype = ctypes.c_int

        # info
        self.lib.smartpot_engine_version.argtypes = []
        self.lib.smartpot_engine_version.restype = ctypes.c_char_p

        self.lib.smartpot_engine_build_info.argtypes = []
        self.lib.smartpot_engine_build_info.restype = ctypes.c_char_p

    @property
    def is_available(self) -> bool:
        return self.lib is not None

    def get_info(self) -> dict[str, Any]:
        if not self.is_available or self.lib is None:
            return {
                "available": False,
                "engine": "Python (NumPy / Scipy fallback)",
                "build": "Standard CPython interpreter",
            }
        return {
            "available": True,
            "engine": self.lib.smartpot_engine_version().decode("utf-8"),
            "build": self.lib.smartpot_engine_build_info().decode("utf-8"),
        }

    def theil_sen(self, hours: np.ndarray, values: np.ndarray) -> tuple[float, float, float]:
        if not self.is_available or self.lib is None:
            raise RuntimeError("Motor nativo C++ no cargado")

        n = len(hours)
        if n == 0:
            return 0.0, 0.0, 0.0

        c_hours = (ctypes.c_double * n)(*hours.astype(np.float64))
        c_values = (ctypes.c_double * n)(*values.astype(np.float64))
        out_res = CTheilSenResult()

        status = self.lib.smartpot_theil_sen(c_hours, c_values, n, ctypes.byref(out_res))
        if status != 0:
            raise ValueError(f"Fallo en smartpot_theil_sen (código {status})")

        return float(out_res.slope), float(out_res.intercept), float(out_res.agreement)

    def fuzzy_health(self, deviations: dict[str, float]) -> dict[str, Any]:
        if not self.is_available or self.lib is None:
            raise RuntimeError("Motor nativo C++ no cargado")

        items = list(deviations.items())
        count = len(items)
        if count == 0:
            return {"index": 0.0, "level": "UNKNOWN", "label": "Sin datos", "by_parameter": {}}

        param_names_bytes = [k.encode("utf-8") for k, _ in items]
        c_names = (ctypes.c_char_p * count)(*param_names_bytes)
        c_devs = (ctypes.c_double * count)(*[float(v) for _, v in items])
        out_res = CHealthResult()

        status = self.lib.smartpot_fuzzy_health_index(c_names, c_devs, count, ctypes.byref(out_res))
        if status != 0:
            raise ValueError(f"Fallo en smartpot_fuzzy_health_index (código {status})")

        by_parameter: dict[str, float] = {}
        for i in range(out_res.param_count):
            p_name = bytes(out_res.param_names[i]).split(b"\0", 1)[0].decode("utf-8")
            by_parameter[p_name] = float(out_res.param_scores[i])

        level_str = bytes(out_res.level).split(b"\0", 1)[0].decode("utf-8")
        label_str = bytes(out_res.label).split(b"\0", 1)[0].decode("utf-8")

        return {
            "index": float(out_res.index),
            "level": level_str,
            "label": label_str,
            "by_parameter": by_parameter,
        }

    def assess_quality(self, matrix: np.ndarray, bounds: list[tuple[float, float]]) -> dict[str, Any]:
        if not self.is_available or self.lib is None:
            raise RuntimeError("Motor nativo C++ no cargado")

        rows, cols = matrix.shape
        flat = matrix.astype(np.float64).flatten()
        c_matrix = (ctypes.c_double * len(flat))(*flat)

        c_bounds = (CPhysicalBound * cols)()
        for i in range(cols):
            c_bounds[i].min_val = float(bounds[i][0])
            c_bounds[i].max_val = float(bounds[i][1])

        out_report = CQualityReport()
        out_keep = (ctypes.c_uint8 * rows)()

        status = self.lib.smartpot_quality_assess(
            c_matrix, rows, cols, c_bounds, ctypes.byref(out_report), out_keep
        )
        if status != 0:
            raise ValueError(f"Fallo en smartpot_quality_assess (código {status})")

        keep_array = np.array(list(out_keep), dtype=bool)
        return {
            "rows": out_report.rows,
            "cols": out_report.cols,
            "completeness": float(out_report.completeness),
            "validity": float(out_report.validity),
            "outliers": float(out_report.outliers),
            "score": float(out_report.score),
            "keep": keep_array,
        }

    def normalize(self, values: list[float], ranges: list[tuple[float, float]]) -> list[float]:
        if not self.is_available or self.lib is None:
            raise RuntimeError("Motor nativo C++ no cargado")

        count = len(values)
        c_vals = (ctypes.c_double * count)(*values)
        c_ranges = (CParamRange * count)()
        for i in range(count):
            c_ranges[i].min_val = ranges[i][0]
            c_ranges[i].max_val = ranges[i][1]

        out_norm = (ctypes.c_double * count)()
        status = self.lib.smartpot_features_normalize(c_vals, c_ranges, count, out_norm)
        if status != 0:
            raise ValueError(f"Fallo en smartpot_features_normalize (código {status})")

        return [float(x) for x in out_norm]

    def circular_hour(self, hour: float) -> tuple[float, float]:
        if not self.is_available or self.lib is None:
            raise RuntimeError("Motor nativo C++ no cargado")

        sin_val = ctypes.c_double()
        cos_val = ctypes.c_double()
        status = self.lib.smartpot_features_circular_hour(hour, ctypes.byref(sin_val), ctypes.byref(cos_val))
        if status != 0:
            raise ValueError(f"Fallo en smartpot_features_circular_hour (código {status})")

        return float(sin_val.value), float(cos_val.value)


# Instancia única del puente nativo
engine = NativeEngine()

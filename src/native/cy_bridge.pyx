# cython: language_level=3
# cython: boundscheck=False
# cython: wraparound=False
# cython: cdivision=True

cimport cy_bridge as cb
from libc.stdint cimport int32_t, uint8_t
from libc.string cimport strncpy
import numpy as np
cimport numpy as cnp

cnp.import_array()

def cy_engine_info():
    cdef const char* ver = cb.smartpot_engine_version()
    cdef const char* bld = cb.smartpot_engine_build_info()
    return {
        "available": True,
        "engine": f"{ver.decode('utf-8')} (Cython nogil)",
        "build": bld.decode("utf-8"),
        "tier": "cython",
    }

def cy_theil_sen(const double[::1] hours, const double[::1] values):
    cdef int count = hours.shape[0]
    if count != values.shape[0]:
        raise ValueError("hours y values deben tener la misma longitud")
    if count == 0:
        return 0.0, 0.0, 0.0

    cdef cb.SmartPotTheilSenResult res
    cdef int status

    with nogil:
        status = cb.smartpot_theil_sen(&hours[0], &values[0], count, &res)

    if status != 0:
        raise RuntimeError(f"Error en smartpot_theil_sen (código {status})")

    return res.slope, res.intercept, res.agreement

def cy_fuzzy_health(dict deviations):
    cdef int count = len(deviations)
    if count == 0:
        return {"index": 0.0, "level": "UNKNOWN", "label": "Sin datos", "by_parameter": {}}

    cdef const char* c_names[16]
    cdef double c_devs[16]
    cdef list name_bytes = []
    cdef int i = 0

    for k, v in deviations.items():
        if i >= 16:
            break
        b = k.encode("utf-8") if isinstance(k, str) else k
        name_bytes.append(b)
        c_names[i] = b
        c_devs[i] = float(v)
        i += 1

    cdef cb.SmartPotHealthResult res
    cdef int status

    with nogil:
        status = cb.smartpot_fuzzy_health_index(c_names, c_devs, i, &res)

    if status != 0:
        raise RuntimeError(f"Error en smartpot_fuzzy_health_index (código {status})")

    cdef dict by_param = {}
    cdef int j
    cdef str p_name

    for j in range(res.param_count):
        p_name = res.param_names[j].decode("utf-8")
        by_param[p_name] = res.param_scores[j]

    return {
        "index": res.index,
        "level": res.level.decode("utf-8"),
        "label": res.label.decode("utf-8"),
        "by_parameter": by_param,
    }

def cy_assess_quality(const double[:, ::1] matrix, list bounds):
    cdef int rows = matrix.shape[0]
    cdef int cols = matrix.shape[1]
    if rows == 0 or cols == 0:
        return {
            "rows": 0, "cols": 0, "completeness": 0.0, "validity": 0.0,
            "outliers": 0.0, "score": 0.0, "keep": np.zeros(0, dtype=bool)
        }

    cdef cb.SmartPotPhysicalBound c_bounds[64]
    cdef int i
    for i in range(min(cols, 64)):
        c_bounds[i].min_val = float(bounds[i][0])
        c_bounds[i].max_val = float(bounds[i][1])

    cdef cb.SmartPotQualityReport report
    cdef cnp.ndarray[cnp.uint8_t, ndim=1] out_keep = np.empty(rows, dtype=np.uint8)
    cdef uint8_t[::1] keep_view = out_keep
    cdef int status

    with nogil:
        status = cb.smartpot_quality_assess(&matrix[0, 0], rows, cols, c_bounds, &report, &keep_view[0])

    if status != 0:
        raise RuntimeError(f"Error en smartpot_quality_assess (código {status})")

    return {
        "rows": report.rows,
        "cols": report.cols,
        "completeness": report.completeness,
        "validity": report.validity,
        "outliers": report.outliers,
        "score": report.score,
        "keep": out_keep.astype(bool),
    }

def cy_circular_hour(double hour):
    cdef double sin_val = 0.0
    cdef double cos_val = 0.0
    cdef int status

    with nogil:
        status = cb.smartpot_features_circular_hour(hour, &sin_val, &cos_val)

    if status != 0:
        raise RuntimeError(f"Error en smartpot_features_circular_hour (código {status})")

    return sin_val, cos_val

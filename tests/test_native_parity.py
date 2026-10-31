import numpy as np

from app.engine.forecast import theil_sen as py_theil_sen
from app.engine.fuzzy import health_index as py_fuzzy
from app.engine.native_bridge import engine


def test_native_engine_is_loaded():
    info = engine.get_info()
    assert info["available"] is True
    assert "SmartPot-AI Native Core" in info["engine"]
    assert "C++20" in info["engine"]


def test_theil_sen_parity_random_datasets():
    rng = np.random.default_rng(42)
    for _ in range(50):
        n = rng.integers(6, 40)
        hours = np.sort(rng.uniform(0, 10, n))
        values = rng.uniform(15, 30, n)

        py_s, py_i, py_a = py_theil_sen(hours, values)
        cpp_s, cpp_i, cpp_a = engine.theil_sen(hours, values)

        assert abs(py_s - cpp_s) < 1e-6, f"Slope mismatch: py={py_s}, cpp={cpp_s}"
        assert abs(py_i - cpp_i) < 1e-6, f"Intercept mismatch: py={py_i}, cpp={cpp_i}"
        assert abs(py_a - cpp_a) < 1e-6, f"Agreement mismatch: py={py_a}, cpp={cpp_a}"


def test_fuzzy_health_parity_random_deviations():
    rng = np.random.default_rng(101)
    for _ in range(50):
        devs = {
            "ph": float(rng.uniform(0, 2.0)),
            "temperature": float(rng.uniform(0, 2.0)),
            "humidity": float(rng.uniform(0, 2.0)),
            "brightness": float(rng.uniform(0, 2.0)),
            "tds": float(rng.uniform(0, 2.0)),
            "soilMoisture": float(rng.uniform(0, 2.0)),
        }
        py_res = py_fuzzy(devs)
        cpp_res = engine.fuzzy_health(devs)

        assert abs(py_res.index - cpp_res["index"]) < 1e-3, f"Index mismatch: {py_res.index} vs {cpp_res['index']}"
        assert py_res.level == cpp_res["level"], f"Level mismatch: {py_res.level} vs {cpp_res['level']}"
        assert py_res.label == cpp_res["label"], f"Label mismatch: {py_res.label} vs {cpp_res['label']}"
        for p in devs:
            assert abs(py_res.by_parameter[p] - cpp_res["by_parameter"][p]) < 1e-3, f"Score mismatch for {p}"


def test_quality_iqr_parity():
    rng = np.random.default_rng(202)
    matrix = np.tile([20.0, 60.0, 900.0, 6.0, 700.0, 1012.0, 70.0], (100, 1)) + rng.normal(0, 0.2, (100, 7))
    matrix[5, 0] = 99.0  # Spikes
    matrix[12, 3] = -5.0

    bounds = [(-10.0, 60.0), (0.0, 100.0), (0.0, 100000.0), (0.0, 14.0), (0.0, 5000.0), (800.0, 1200.0), (0.0, 100.0)]
    res = engine.assess_quality(matrix, bounds)

    assert not res["keep"][5]
    assert not res["keep"][12]
    assert res["score"] > 0.95
    assert res["validity"] < 1.0


def test_circular_hour_encoding():
    for h in [0.0, 6.0, 12.0, 18.0, 23.5]:
        sin_cpp, cos_cpp = engine.circular_hour(h)
        angle = 2.0 * np.pi * h / 24.0
        assert abs(sin_cpp - np.sin(angle)) < 1e-9
        assert abs(cos_cpp - np.cos(angle)) < 1e-9

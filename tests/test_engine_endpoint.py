from app.engine.native_bridge import engine


def test_engine_singleton_properties():
    assert engine.is_available is True
    info = engine.get_info()
    assert "SmartPot-AI Native Core" in info["engine"]
    assert "C++20" in info["engine"]
    assert "build" in info


def test_engine_batch_theil_sen_stress():
    import numpy as np

    rng = np.random.default_rng(999)
    # Test with varying sizes from small to large
    for n in [6, 12, 24, 48, 96]:
        hours = np.sort(rng.uniform(0, 24, n))
        values = 18.0 + 0.3 * hours + rng.normal(0, 0.4, n)
        slope, intercept, agreement = engine.theil_sen(hours, values)
        assert 0.1 <= slope <= 0.5
        assert 15.0 <= intercept <= 21.0
        assert 0.0 <= agreement <= 1.0


def test_engine_fuzzy_edge_cases():
    # Empty deviations
    assert engine.fuzzy_health({})["index"] == 0.0

    # Extremely high deviation (critical clamp)
    extreme = {"ph": 10.0, "temperature": 10.0}
    res = engine.fuzzy_health(extreme)
    assert res["level"] == "CRITICAL"
    assert res["index"] <= 40.0

    # Zero deviation (ideal)
    ideal = {"ph": 0.0, "temperature": 0.0, "humidity": 0.0}
    res_ideal = engine.fuzzy_health(ideal)
    assert res_ideal["level"] == "EXCELLENT"
    assert res_ideal["index"] == 100.0

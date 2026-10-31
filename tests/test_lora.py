import time

import numpy as np

from app.learning.lora import LowRankCropAdapter


def test_lora_initial_state_matches_w0():
    adapter = LowRankCropAdapter(d_in=10, k_out=1, rank=2, alpha=1.0, seed=42)
    assert np.allclose(adapter.b, 0.0)
    assert np.allclose(adapter.effective_weight, adapter.w0)
    assert np.allclose(adapter.effective_bias, adapter.b0)


def test_lora_predict_proba_shapes_and_values():
    adapter = LowRankCropAdapter(d_in=10, k_out=1, rank=3, alpha=2.0)
    x = np.random.default_rng(0).normal(0.5, 0.2, (50, 10))
    proba = adapter.predict_proba(x)

    assert proba.shape == (50, 2)
    assert np.all(proba >= 0.0)
    assert np.all(proba <= 1.0)
    assert np.allclose(proba.sum(axis=1), 1.0)


def test_lora_online_update_sub_millisecond():
    adapter = LowRankCropAdapter(d_in=10, k_out=1, rank=4, alpha=1.5, seed=123)
    rng = np.random.default_rng(42)
    x_batch = rng.normal(0.0, 1.0, (32, 10))
    y_batch = (x_batch[:, 0] > 0.0).astype(int)

    # Medir latencia de actualización online
    t0 = time.perf_counter()
    loss = adapter.online_update(x_batch, y_batch, lr=0.05)
    duration_ms = (time.perf_counter() - t0) * 1000.0

    assert duration_ms < 1.0, f"Online update took {duration_ms:.3f} ms, expected < 1 ms"
    assert adapter.updates_count == 1
    assert not np.allclose(adapter.b, 0.0)
    assert loss > 0.0


def test_lora_loss_decreases_with_updates():
    adapter = LowRankCropAdapter(d_in=6, k_out=1, rank=2, alpha=1.0, seed=99)
    rng = np.random.default_rng(77)
    x = rng.normal(0.0, 1.0, (100, 6))
    y = (x[:, 1] + x[:, 2] > 0.5).astype(int)

    initial_loss = adapter.online_update(x, y, lr=0.0)
    for _ in range(20):
        loss = adapter.online_update(x, y, lr=0.1)

    assert loss < initial_loss, f"Loss did not decrease: initial={initial_loss}, final={loss}"


def test_lora_serialization_parity():
    adapter = LowRankCropAdapter(d_in=10, k_out=1, rank=2, alpha=1.0, seed=10)
    x = np.random.default_rng(1).normal(0.0, 1.0, (20, 10))
    y = np.ones(20, dtype=int)
    adapter.online_update(x, y, lr=0.05)

    data = adapter.to_dict()
    restored = LowRankCropAdapter.from_dict(data)

    p_orig = adapter.predict_proba(x)
    p_rest = restored.predict_proba(x)
    assert np.allclose(p_orig, p_rest, atol=1e-9)

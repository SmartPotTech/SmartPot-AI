import numpy as np

from app.engine.ekf_filter import SensorEKF


def test_ekf_filter_noise_attenuation():
    rng = np.random.default_rng(42)
    true_val = 25.0
    # Ruido gaussiano con sigma = 1.0
    noisy_measurements = true_val + rng.normal(0.0, 1.0, size=50)

    ekf = SensorEKF(initial_value=25.0, process_variance=1e-4, measurement_variance=1.0)
    filtered = []
    for m in noisy_measurements:
        est, accepted = ekf.update(float(m))
        assert accepted is True
        filtered.append(est)

    # La varianza de los valores filtrados debe ser menor que la de las mediciones ruidosas
    assert np.var(filtered[20:]) < np.var(noisy_measurements[20:])
    assert abs(filtered[-1] - true_val) < 0.5


def test_ekf_innovation_gating_rejects_glitch():
    ekf = SensorEKF(initial_value=6.0, process_variance=1e-4, measurement_variance=0.05, innovation_gate_sigma=4.0)

    # Alimentamos 10 lecturas normales de pH ~ 6.0
    for _ in range(10):
        est, accepted = ekf.update(6.0)
        assert accepted is True

    # Lectura espuria (cable suelto temporalmente pH 14.0)
    est_glitch, accepted_glitch = ekf.update(14.0)
    assert accepted_glitch is False
    assert est_glitch < 7.0  # El estado estimado no saltó a 14.0
    assert ekf.state.rejected_glitches == 1

import numpy as np

from app.engine.foliar_vpd import batch_compute_vpd_simd, compute_foliar_vpd, saturation_vapor_pressure_buck


def test_saturation_vapor_pressure_buck_values():
    # A 20 °C, e_s debe rondar los 2.34 kPa
    es_20 = saturation_vapor_pressure_buck(20.0)
    assert 2.30 <= es_20 <= 2.38

    # A 30 °C, e_s debe rondar los 4.24 kPa
    es_30 = saturation_vapor_pressure_buck(30.0)
    assert 4.15 <= es_30 <= 4.35


def test_compute_foliar_vpd_optimal_range():
    # 24 °C y 65% HR con foliar -1.5 °C produce un VPD foliar óptimo (~0.8 - 1.1 kPa)
    res = compute_foliar_vpd(air_temp_c=24.0, relative_humidity=65.0, delta_t_foliar=-1.5)
    assert res.air_temperature == 24.0
    assert res.foliar_temperature == 22.5
    assert res.transpiration_status in ("OPTIMAL", "ACCEPTABLE")
    assert 0.6 <= res.vpd_foliar_kpa <= 1.3


def test_compute_foliar_vpd_extremes():
    # Humedad altísima (98%) y frío -> baja transpiración
    res_low = compute_foliar_vpd(air_temp_c=18.0, relative_humidity=98.0)
    assert res_low.transpiration_status == "LOW_TRANSPIRATION"

    # Calor (38 °C) y aire seco (20%) -> estrés excesivo
    res_high = compute_foliar_vpd(air_temp_c=38.0, relative_humidity=20.0)
    assert res_high.transpiration_status in ("EXCESSIVE", "HIGH_STRESS")


def test_batch_compute_vpd_simd_parity():
    temps = np.array([15.0, 20.0, 25.0, 30.0, 35.0])
    rhs = np.array([40.0, 50.0, 60.0, 70.0, 80.0])

    foliar_batch, air_batch = batch_compute_vpd_simd(temps, rhs, delta_t_foliar=-1.5)

    assert len(foliar_batch) == 5
    assert len(air_batch) == 5

    for i in range(5):
        single = compute_foliar_vpd(float(temps[i]), float(rhs[i]), delta_t_foliar=-1.5)
        np.testing.assert_allclose(foliar_batch[i], single.vpd_foliar_kpa, atol=1e-3)
        np.testing.assert_allclose(air_batch[i], single.vpd_air_kpa, atol=1e-3)

"""Psicrometría foliar de alta precisión y cálculo de VPD (Vapor Pressure Deficit).

Implementa la ecuación de Buck (1981) con corrección de temperatura foliar:
    T_foliar = T_aire + Delta_T_foliar (Delta_T_foliar tipicamente -1.5 °C en transpiración activa).
    VPD_foliar = e_s(T_foliar) - e_a(T_aire, RH)
Vectorizado para ejecución SIMD en procesadores modernos.
"""

from dataclasses import dataclass

import numpy as np


@dataclass(frozen=True)
class FoliarVPDResult:
    air_temperature: float
    foliar_temperature: float
    relative_humidity: float
    vapor_pressure_air: float
    vapor_pressure_saturation_leaf: float
    vpd_foliar_kpa: float
    vpd_air_kpa: float
    transpiration_status: str  # OPTIMAL, LOW_TRANSPIRATION, HIGH_STRESS, EXCESSIVE


def saturation_vapor_pressure_buck(temperature_c: np.ndarray | float) -> np.ndarray | float:
    """Calcula la presión de vapor de saturación e_s (kPa) usando la formulación de Buck (1981).

    e_s(T) = 0.61121 * exp((18.678 - T / 234.5) * (T / (257.14 + T)))
    """
    t = np.asarray(temperature_c, dtype=np.float64)
    es = 0.61121 * np.exp((18.678 - t / 234.5) * (t / (257.14 + t)))
    return es if isinstance(temperature_c, np.ndarray) else float(es)


def compute_foliar_vpd(
    air_temp_c: float,
    relative_humidity: float,
    delta_t_foliar: float = -1.5,
) -> FoliarVPDResult:
    """Calcula el VPD foliar y clasifica el estado de transpiración estomática de la planta."""
    t_air = float(air_temp_c)
    rh = float(np.clip(relative_humidity, 0.0, 100.0))
    t_leaf = t_air + float(delta_t_foliar)

    es_leaf = float(saturation_vapor_pressure_buck(t_leaf))
    es_air = float(saturation_vapor_pressure_buck(t_air))
    ea_air = float(es_air * (rh / 100.0))

    vpd_foliar = float(max(0.0, es_leaf - ea_air))
    vpd_air = float(max(0.0, es_air - ea_air))

    # Clasificación agronómica de VPD foliar (en kPa)
    if vpd_foliar < 0.4:
        status = "LOW_TRANSPIRATION"  # Riesgo de condensación y hongos (Botrytis)
    elif 0.8 <= vpd_foliar <= 1.2:
        status = "OPTIMAL"  # Máxima eficiencia fotosintética y transporte de calcio
    elif vpd_foliar > 1.6:
        status = "EXCESSIVE"  # Cierre estomático por sequedad excesiva
    else:
        status = "HIGH_STRESS" if vpd_foliar > 1.2 else "ACCEPTABLE"

    return FoliarVPDResult(
        air_temperature=t_air,
        foliar_temperature=t_leaf,
        relative_humidity=rh,
        vapor_pressure_air=ea_air,
        vapor_pressure_saturation_leaf=es_leaf,
        vpd_foliar_kpa=round(vpd_foliar, 3),
        vpd_air_kpa=round(vpd_air, 3),
        transpiration_status=status,
    )


def batch_compute_vpd_simd(
    temps: np.ndarray,
    rhs: np.ndarray,
    delta_t_foliar: float = -1.5,
) -> tuple[np.ndarray, np.ndarray]:
    """Cálculo vectorizado SIMD por lotes de VPD foliar y VPD de aire para series de telemetría."""
    t_air = np.asarray(temps, dtype=np.float64)
    rh = np.clip(np.asarray(rhs, dtype=np.float64), 0.0, 100.0)
    t_leaf = t_air + delta_t_foliar

    es_leaf = saturation_vapor_pressure_buck(t_leaf)
    es_air = saturation_vapor_pressure_buck(t_air)
    ea_air = es_air * (rh / 100.0)

    vpd_foliar = np.maximum(0.0, es_leaf - ea_air)
    vpd_air = np.maximum(0.0, es_air - ea_air)

    return vpd_foliar, vpd_air

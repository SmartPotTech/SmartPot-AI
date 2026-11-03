//! Déficit de presión de vapor (VPD) foliar con la ecuación de Buck (1981).
//!
//! `T_hoja = T_aire + ΔT` (ΔT ≈ -1,5 °C con transpiración activa) y `VPD_hoja = e_s(T_hoja) - e_a(T_aire, HR)`.
//! Es una utilidad del motor: el contrato de la evaluación no la expone para mantener la paridad con el servicio en
//! Python.

/// Diferencia típica entre la hoja que transpira y el aire.
pub const DEFAULT_LEAF_OFFSET: f64 = -1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transpiration {
    /// Menos de 0,4 kPa: riesgo de condensación y hongos.
    Low,
    /// 0,4 a 0,8 kPa.
    Acceptable,
    /// 0,8 a 1,2 kPa: máxima eficiencia.
    Optimal,
    /// 1,2 a 1,6 kPa.
    HighStress,
    /// Más de 1,6 kPa: cierre estomático.
    Excessive,
}

#[derive(Debug, Clone, Copy)]
pub struct FoliarVpd {
    pub leaf_temperature: f64,
    pub vapor_pressure_air: f64,
    pub saturation_leaf: f64,
    pub vpd_leaf_kpa: f64,
    pub vpd_air_kpa: f64,
    pub transpiration: Transpiration,
}

/// Presión de vapor de saturación en kPa: `0,61121 · exp((18,678 - T/234,5) · T / (257,14 + T))`.
pub fn saturation_vapor_pressure(temperature_c: f64) -> f64 {
    0.61121 * ((18.678 - temperature_c / 234.5) * (temperature_c / (257.14 + temperature_c))).exp()
}

pub fn foliar_vpd(air_temperature: f64, relative_humidity: f64, leaf_offset: f64) -> FoliarVpd {
    let humidity = relative_humidity.clamp(0.0, 100.0);
    let leaf = air_temperature + leaf_offset;
    let saturation_leaf = saturation_vapor_pressure(leaf);
    let saturation_air = saturation_vapor_pressure(air_temperature);
    let actual = saturation_air * (humidity / 100.0);
    let vpd_leaf = (saturation_leaf - actual).max(0.0);
    let vpd_air = (saturation_air - actual).max(0.0);
    let transpiration = if vpd_leaf < 0.4 {
        Transpiration::Low
    } else if (0.8..=1.2).contains(&vpd_leaf) {
        Transpiration::Optimal
    } else if vpd_leaf > 1.6 {
        Transpiration::Excessive
    } else if vpd_leaf > 1.2 {
        Transpiration::HighStress
    } else {
        Transpiration::Acceptable
    };
    FoliarVpd {
        leaf_temperature: leaf,
        vapor_pressure_air: actual,
        saturation_leaf,
        vpd_leaf_kpa: crate::format::py_round(vpd_leaf, 3),
        vpd_air_kpa: crate::format::py_round(vpd_air, 3),
        transpiration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturation_matches_reference_values() {
        // Valores de referencia de Buck: 0 °C ≈ 0,6112 kPa, 20 °C ≈ 2,3389 kPa, 30 °C ≈ 4,2460 kPa.
        assert!((saturation_vapor_pressure(0.0) - 0.61121).abs() < 1e-5);
        assert!((saturation_vapor_pressure(20.0) - 2.3389).abs() < 1e-3);
        assert!((saturation_vapor_pressure(30.0) - 4.2460).abs() < 2e-3);
    }

    #[test]
    fn classifies_transpiration() {
        assert_eq!(
            foliar_vpd(22.0, 95.0, DEFAULT_LEAF_OFFSET).transpiration,
            Transpiration::Low
        );
        assert_eq!(
            foliar_vpd(25.0, 65.0, DEFAULT_LEAF_OFFSET).transpiration,
            Transpiration::Optimal
        );
        assert_eq!(
            foliar_vpd(35.0, 20.0, DEFAULT_LEAF_OFFSET).transpiration,
            Transpiration::Excessive
        );
        let saturated = foliar_vpd(20.0, 100.0, 0.0);
        assert_eq!(saturated.vpd_air_kpa, 0.0);
    }
}

//! Variables de entrada de los modelos aprendidos para la lectura actual (las mismas que usa SmartPot-Trainer).
//!
//! Seis posiciones dentro del rango ideal (0 = mínimo, 1 = máximo), la hora del día en forma circular (seno y coseno)
//! y la tendencia de la última media hora del sustrato y de la temperatura, en rangos ideales por hora.

use crate::domain::{CropProfile, Parameter, Values};

pub const FEATURES: usize = 10;
pub const SLOPE_WINDOW_SECONDS: f64 = 1800.0;
pub const MIN_SLOPE_SPAN_SECONDS: f64 = 600.0;

/// Cambio por hora de la última lectura frente a la primera de la ventana; 0 si no hay historial suficiente.
fn last_slope(times: &[f64], series: &[f64], scale: f64) -> f64 {
    let n = times.len();
    let last = n - 1;
    let from = times[last] - SLOPE_WINDOW_SECONDS;
    let start = times.partition_point(|t| *t < from);
    let span = times[last] - times[start];
    if span >= MIN_SLOPE_SPAN_SECONDS && !series[last].is_nan() && !series[start].is_nan() {
        (series[last] - series[start]) / (scale * (span / 3600.0))
    } else {
        0.0
    }
}

/// Variables de la lectura actual; `None` si falta alguna de las seis variables con rango.
///
/// `moment` y las horas del historial son segundos epoch; `hour` es la hora local (sin ella se usa la hora UTC).
pub fn live_features(
    profile: &CropProfile,
    values: &Values,
    hour: Option<u8>,
    moment: f64,
    history: &[(f64, Values)],
) -> Option<[f64; FEATURES]> {
    let mut features = [0.0; FEATURES];
    for parameter in Parameter::ALL {
        let value = values[parameter.index()]?;
        let range = profile.range(parameter);
        features[parameter.index()] = (value - range.min) / (range.max - range.min);
    }

    let mut timeline: Vec<&(f64, Values)> = history.iter().filter(|(t, _)| *t < moment).collect();
    timeline.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut times: Vec<f64> = timeline.iter().map(|(t, _)| *t).collect();
    times.push(moment);
    let series = |parameter: Parameter| -> Vec<f64> {
        let mut out: Vec<f64> = timeline
            .iter()
            .map(|(_, m)| m[parameter.index()].unwrap_or(f64::NAN))
            .collect();
        out.push(values[parameter.index()].unwrap_or(f64::NAN));
        out
    };

    let local = match hour {
        Some(h) => f64::from(h),
        None => (moment / 3600.0).rem_euclid(24.0),
    };
    let angle = local / 24.0 * 2.0 * std::f64::consts::PI;
    features[6] = angle.sin();
    features[7] = angle.cos();
    let soil = profile.range(Parameter::SoilMoisture);
    let temperature = profile.range(Parameter::Temperature);
    features[8] = last_slope(
        &times,
        &series(Parameter::SoilMoisture),
        soil.max - soil.min,
    );
    features[9] = last_slope(
        &times,
        &series(Parameter::Temperature),
        temperature.max - temperature.min,
    );
    Some(features)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile;

    fn full(soil: f64, temperature: f64) -> Values {
        [
            Some(temperature),
            Some(60.0),
            Some(800.0),
            Some(6.0),
            Some(700.0),
            Some(soil),
        ]
    }

    #[test]
    fn needs_every_variable() {
        let lettuce = profile("LETTUCE").unwrap();
        let mut partial = full(70.0, 18.0);
        partial[Parameter::Ph.index()] = None;
        assert!(live_features(lettuce, &partial, Some(12), 1e9, &[]).is_none());
    }

    #[test]
    fn measures_the_recent_trend() {
        let lettuce = profile("LETTUCE").unwrap();
        let now = 1_800_000_000.0;
        let history = vec![
            (now - 1200.0, full(72.0, 18.0)),
            (now - 4000.0, full(90.0, 18.0)),
        ];
        let x = live_features(lettuce, &full(70.0, 18.0), Some(6), now, &history).unwrap();
        assert!((x[5] - 0.5).abs() < 1e-12);
        assert!((x[6] - 1.0).abs() < 1e-12 && x[7].abs() < 1e-12);
        // De 72 % a 70 % en 20 minutos: -6 % por hora sobre un rango de 20 puntos.
        assert!((x[8] - (-0.3)).abs() < 1e-12);
        assert_eq!(x[9], 0.0);
    }
}

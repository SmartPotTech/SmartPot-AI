//! Pronóstico de tendencia por variable a partir del historial con su hora de medición.
//!
//! Se ajusta una recta con el estimador de Theil-Sen (mediana de las pendientes entre pares de lecturas), que tolera
//! lecturas atípicas mejor que los mínimos cuadrados. La mediana se obtiene por selección parcial
//! (`select_nth_unstable_by`), en O(M) sobre las M pendientes, sin ordenarlas. Con la pendiente por hora se estima el
//! valor dentro de 3 horas y, si la tendencia empuja la variable fuera de su rango ideal, en cuántas horas cruzará el
//! límite. La luz no se pronostica: sigue el ciclo del día.

use crate::domain::{CropProfile, Parameter, Values};
use crate::format::{capitalize, py_round, py_round_int, short};
use crate::numeric::{median, sign};
use crate::schema::Micros;

pub const FORECAST_PARAMETERS: [Parameter; 5] = [
    Parameter::Temperature,
    Parameter::Humidity,
    Parameter::Ph,
    Parameter::Tds,
    Parameter::SoilMoisture,
];
pub const MIN_POINTS: usize = 6;
pub const MIN_SPAN_HOURS: f64 = 0.3;
pub const HORIZON_HOURS: f64 = 3.0;
pub const MAX_HOURS_TO_LIMIT: f64 = 24.0;
/// Una variable es estable si en 3 h no se mueve más del 10 % del ancho de su rango ideal.
pub const STABLE_SHARE: f64 = 0.10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

impl Trend {
    pub fn name(self) -> &'static str {
        match self {
            Trend::Rising => "RISING",
            Trend::Falling => "FALLING",
            Trend::Stable => "STABLE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Max,
    Min,
}

impl Limit {
    pub fn name(self) -> &'static str {
        match self {
            Limit::Max => "MAX",
            Limit::Min => "MIN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Forecast {
    pub parameter: Parameter,
    pub current: f64,
    pub slope_per_hour: f64,
    pub expected_in_3h: f64,
    pub trend: Trend,
    pub hours_to_limit: Option<f64>,
    pub limit: Option<Limit>,
    pub confidence: f64,
    pub message: String,
}

/// Pendiente, intercepto y concordancia (fracción de pendientes con el signo de la mediana).
pub fn theil_sen(hours: &[f64], values: &[f64]) -> (f64, f64, f64) {
    let n = hours.len();
    let mut slopes = Vec::with_capacity(n.saturating_sub(1) * n / 2);
    for i in 0..n {
        for j in (i + 1)..n {
            let dt = hours[j] - hours[i];
            if dt > 1e-9 {
                slopes.push((values[j] - values[i]) / dt);
            }
        }
    }
    if slopes.is_empty() {
        let mut copy = values.to_vec();
        return (0.0, median(&mut copy), 0.0);
    }
    let mut scratch = slopes.clone();
    let slope = median(&mut scratch);
    let mut residuals: Vec<f64> = values
        .iter()
        .zip(hours)
        .map(|(v, h)| v - slope * h)
        .collect();
    let intercept = median(&mut residuals);
    let agreement = if slope != 0.0 {
        let target = sign(slope);
        slopes.iter().filter(|s| sign(**s) == target).count() as f64 / slopes.len() as f64
    } else {
        1.0
    };
    (slope, intercept, agreement)
}

/// Pronóstico de cada variable a partir de las lecturas con hora (microsegundos UTC).
pub fn forecast(profile: &CropProfile, history: &[(Micros, Values)]) -> Vec<Forecast> {
    let mut points: Vec<&(Micros, Values)> = history.iter().collect();
    points.sort_by_key(|(moment, _)| *moment);
    if points.len() < MIN_POINTS {
        return Vec::new();
    }
    let origin = points[points.len() - 1].0;
    let mut results = Vec::new();
    for parameter in FORECAST_PARAMETERS {
        let mut hours = Vec::with_capacity(points.len());
        let mut values = Vec::with_capacity(points.len());
        for (moment, measures) in &points {
            if let Some(value) = measures[parameter.index()] {
                hours.push((*moment - origin) as f64 / 1e6 / 3600.0);
                values.push(value);
            }
        }
        if values.len() < MIN_POINTS {
            continue;
        }
        let max = hours.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let min = hours.iter().copied().fold(f64::INFINITY, f64::min);
        if max - min < MIN_SPAN_HOURS {
            continue;
        }
        let (slope, intercept, agreement) = theil_sen(&hours, &values);
        results.push(describe(
            profile,
            parameter,
            values[values.len() - 1],
            slope,
            intercept,
            agreement,
        ));
    }
    results
}

fn describe(
    profile: &CropProfile,
    parameter: Parameter,
    current: f64,
    slope: f64,
    intercept: f64,
    agreement: f64,
) -> Forecast {
    let range = profile.range(parameter);
    let width = range.width();
    let expected = intercept + slope * HORIZON_HOURS;
    let mut trend = Trend::Stable;
    if slope.abs() * HORIZON_HOURS >= STABLE_SHARE * width {
        trend = if slope > 0.0 {
            Trend::Rising
        } else {
            Trend::Falling
        };
    }

    // Las horas al límite parten del nivel ajustado (no de la última lectura, que trae ruido), igual que el valor
    // esperado en 3 h: así ambos datos son coherentes entre sí.
    let level = intercept;
    let mut hours_to_limit = None;
    let mut limit = None;
    if trend != Trend::Stable && range.min <= level && level <= range.max {
        let hours = if slope > 0.0 {
            (range.max - level) / slope
        } else {
            (level - range.min) / -slope
        };
        if hours <= MAX_HOURS_TO_LIMIT {
            hours_to_limit = Some(py_round(hours, 2));
            limit = Some(if slope > 0.0 { Limit::Max } else { Limit::Min });
        }
    }

    let label = if parameter == Parameter::Ph {
        "pH".to_string()
    } else {
        capitalize(range.label)
    };
    let per_hour = format!("{} {} per hour", short(slope.abs()), range.unit);
    let verb = if slope > 0.0 {
        "is rising"
    } else {
        "is falling"
    };
    let message = match (trend, hours_to_limit, limit) {
        (Trend::Stable, _, _) => format!("{label} remains stable."),
        (_, Some(hours), Some(bound)) => {
            let (value, word) = if bound == Limit::Max {
                (range.max, "maximum")
            } else {
                (range.min, "minimum")
            };
            format!(
                "{label} {verb} {per_hour}: it will reach the {word} ({} {}) in {}.",
                short(value),
                range.unit,
                hours_text(hours)
            )
        }
        _ => format!("{label} {verb} {per_hour}."),
    };

    Forecast {
        parameter,
        current: py_round(current, 2),
        slope_per_hour: py_round(slope, 3),
        expected_in_3h: py_round(expected, 2),
        trend,
        hours_to_limit,
        limit,
        confidence: py_round(agreement, 2),
        message,
    }
}

fn hours_text(value: f64) -> String {
    if value < 1.0 {
        let minutes = (py_round_int(value * 60.0 / 5.0) * 5).max(5);
        return format!("about {minutes} minutes");
    }
    if value >= 1.5 {
        format!("about {} h", py_round_int(value))
    } else {
        "around 1 h".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile;

    fn naive_theil_sen(hours: &[f64], values: &[f64]) -> f64 {
        let mut slopes = Vec::new();
        for i in 0..hours.len() {
            for j in (i + 1)..hours.len() {
                if hours[j] - hours[i] > 1e-9 {
                    slopes.push((values[j] - values[i]) / (hours[j] - hours[i]));
                }
            }
        }
        slopes.sort_by(f64::total_cmp);
        let n = slopes.len();
        if n % 2 == 1 {
            slopes[n / 2]
        } else {
            (slopes[n / 2 - 1] + slopes[n / 2]) / 2.0
        }
    }

    #[test]
    fn selection_median_matches_full_sort() {
        let hours: Vec<f64> = (0..40).map(|i| f64::from(i) * 0.25).collect();
        let values: Vec<f64> = hours
            .iter()
            .enumerate()
            .map(|(i, h)| 20.0 + 0.8 * h + ((i * 7919) % 13) as f64 * 0.1)
            .collect();
        let (slope, _, _) = theil_sen(&hours, &values);
        assert_eq!(slope, naive_theil_sen(&hours, &values));
    }

    #[test]
    fn tolerates_an_outlier() {
        let hours: Vec<f64> = (0..10).map(f64::from).collect();
        let mut values: Vec<f64> = hours.iter().map(|h| 10.0 + 2.0 * h).collect();
        values[4] = 500.0;
        let (slope, intercept, agreement) = theil_sen(&hours, &values);
        assert!((slope - 2.0).abs() < 1e-9);
        assert!((intercept - 10.0).abs() < 1e-9);
        assert!(agreement > 0.7);
    }

    #[test]
    fn drying_soil_reaches_the_minimum() {
        let lettuce = profile("LETTUCE").unwrap();
        let history: Vec<(Micros, Values)> = (0..8)
            .map(|i| {
                let mut values = [None; 6];
                values[Parameter::SoilMoisture.index()] = Some(80.0 - f64::from(i));
                (i64::from(i) * 1_800_000_000, values)
            })
            .collect();
        let result = forecast(lettuce, &history);
        assert_eq!(result.len(), 1);
        let soil = &result[0];
        assert_eq!(soil.trend, Trend::Falling);
        assert_eq!(soil.limit, Some(Limit::Min));
        assert!(soil.message.starts_with(
            "Substrate moisture is falling 2 % per hour: it will reach the minimum (60 %) in about 6 h."
        ));
    }

    #[test]
    fn needs_six_points_over_eighteen_minutes() {
        let basil = profile("BASIL").unwrap();
        let short_history: Vec<(Micros, Values)> = (0..6)
            .map(|i| {
                (
                    i64::from(i) * 60_000_000,
                    [Some(25.0), None, None, None, None, None],
                )
            })
            .collect();
        assert!(forecast(basil, &short_history).is_empty());
    }

    #[test]
    fn theil_sen_handles_empty_input_without_panicking() {
        assert_eq!(theil_sen(&[], &[]), (0.0, 0.0, 0.0));
    }
}

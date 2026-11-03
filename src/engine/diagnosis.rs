//! Diagnóstico por variable: compara cada lectura con el perfil del cultivo.
//!
//! Entre las 22:00 y las 6:00 (hora local) la planta descansa: la luz baja es normal y se marca como REST.

use crate::domain::{CropProfile, Parameter, Values};
use crate::format::{capitalize, short};

pub const CRITICAL_DEVIATION: f64 = 1.0;
pub const REST_START_HOUR: u8 = 22;
pub const REST_END_HOUR: u8 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Optimal,
    Low,
    High,
    Rest,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Optimal => "OPTIMAL",
            Status::Low => "LOW",
            Status::High => "HIGH",
            Status::Rest => "REST",
        }
    }

    /// Fuera de rango (LOW o HIGH).
    pub fn is_out(self) -> bool {
        matches!(self, Status::Low | Status::High)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Ok,
    Warning,
    Critical,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Ok => "OK",
            Severity::Warning => "WARNING",
            Severity::Critical => "CRITICAL",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParameterDiagnosis {
    pub parameter: Parameter,
    pub value: f64,
    pub status: Status,
    pub severity: Severity,
    pub deviation: f64,
    pub message: String,
    pub recommendation: Option<&'static str>,
}

fn recommendation(parameter: Parameter, status: Status) -> Option<&'static str> {
    use Parameter::*;
    use Status::*;
    Some(match (parameter, status) {
        (Temperature, Low) => {
            "Aleja el cultivo de corrientes frías o usa una fuente de calor suave."
        }
        (Temperature, High) => {
            "Ventila el espacio y evita la luz directa en las horas de más calor."
        }
        (Humidity, Low) => {
            "Aumenta la humedad del ambiente con un humidificador o agrupando plantas."
        }
        (Humidity, High) => "Mejora la circulación de aire para prevenir hongos.",
        (Brightness, Low) => {
            "Enciende la luz ultravioleta o acerca el cultivo a una fuente de luz."
        }
        (Brightness, High) => "Reduce las horas de luz o aleja la lámpara para evitar quemaduras.",
        (Ph, Low) => "Agrega solución para subir el pH (pH Up) en dosis pequeñas.",
        (Ph, High) => "Agrega solución para bajar el pH (pH Down) en dosis pequeñas.",
        (Tds, Low) => "Agrega solución nutritiva A y B según la etapa del cultivo.",
        (Tds, High) => "Diluye la solución con agua limpia para bajar la concentración de sales.",
        (SoilMoisture, Low) => "Riega hasta humedecer el sustrato sin encharcarlo.",
        (SoilMoisture, High) => {
            "Suspende el riego y revisa el drenaje para evitar la pudrición de raíces."
        }
        _ => return None,
    })
}

/// Descanso nocturno: de 22:00 a 6:00 hora local.
pub fn is_rest_hour(hour: Option<u8>) -> bool {
    matches!(hour, Some(h) if !(REST_END_HOUR..REST_START_HOUR).contains(&h))
}

pub fn diagnose(
    profile: &CropProfile,
    values: &Values,
    local_hour: Option<u8>,
) -> Vec<ParameterDiagnosis> {
    let resting = is_rest_hour(local_hour);
    let mut results = Vec::with_capacity(6);
    for parameter in Parameter::ALL {
        let Some(value) = values[parameter.index()] else {
            continue;
        };
        let range = profile.range(parameter);
        let mut deviation = range.deviation(value);
        let (status, severity, message);
        if resting && parameter == Parameter::Brightness && value < range.min {
            status = Status::Rest;
            severity = Severity::Ok;
            deviation = 0.0;
            message = format!(
                "Es de noche: la planta descansa y la luz baja ({} {}) es normal entre las {REST_START_HOUR}:00 y las {REST_END_HOUR}:00.",
                short(value),
                range.unit
            );
        } else if deviation == 0.0 {
            status = Status::Optimal;
            severity = Severity::Ok;
            message = format!(
                "{} está en el rango ideal ({}–{} {}).",
                capitalize(range.label),
                short(range.min),
                short(range.max),
                range.unit
            );
        } else {
            status = if value < range.min {
                Status::Low
            } else {
                Status::High
            };
            severity = if deviation >= CRITICAL_DEVIATION {
                Severity::Critical
            } else {
                Severity::Warning
            };
            let direction = if status == Status::Low {
                "por debajo"
            } else {
                "por encima"
            };
            message = format!(
                "{} ({} {}) está {direction} del rango ideal ({}–{} {}).",
                capitalize(range.label),
                short(value),
                range.unit,
                short(range.min),
                short(range.max),
                range.unit
            );
        }
        results.push(ParameterDiagnosis {
            parameter,
            value,
            status,
            severity,
            deviation: crate::format::py_round(deviation, 3),
            message,
            recommendation: recommendation(parameter, status),
        });
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile;

    fn values(pairs: &[(Parameter, f64)]) -> Values {
        let mut v = [None; 6];
        for &(p, x) in pairs {
            v[p.index()] = Some(x);
        }
        v
    }

    #[test]
    fn classifies_low_high_and_optimal() {
        let lettuce = profile("LETTUCE").unwrap();
        let found = diagnose(
            lettuce,
            &values(&[
                (Parameter::Temperature, 30.0),
                (Parameter::Humidity, 60.0),
                (Parameter::SoilMoisture, 50.0),
            ]),
            Some(12),
        );
        assert_eq!(found[0].status, Status::High);
        assert_eq!(found[0].severity, Severity::Critical);
        assert_eq!(found[1].status, Status::Optimal);
        assert_eq!(found[2].status, Status::Low);
        assert_eq!(found[2].severity, Severity::Warning);
        assert_eq!(
            found[0].message,
            "La temperatura (30 °C) está por encima del rango ideal (15–22 °C)."
        );
    }

    #[test]
    fn low_light_at_night_is_rest() {
        let tomato = profile("TOMATO").unwrap();
        for hour in [22, 23, 0, 5] {
            let found = diagnose(tomato, &values(&[(Parameter::Brightness, 0.0)]), Some(hour));
            assert_eq!(found[0].status, Status::Rest, "hora {hour}");
            assert_eq!(found[0].deviation, 0.0);
        }
        let day = diagnose(tomato, &values(&[(Parameter::Brightness, 0.0)]), Some(6));
        assert_eq!(day[0].status, Status::Low);
    }
}

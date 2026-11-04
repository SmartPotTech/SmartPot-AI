//! Agente reactivo simple: percepción (diagnóstico) → reglas de acción → acciones.
//!
//! No guarda estado entre lecturas; la API aplica el enfriamiento por actuador y solo ejecuta acciones si el cultivo
//! tiene el modo automático activo. De 22:00 a 6:00 (hora local) respeta el descanso de la planta: nunca enciende la
//! luz ultravioleta y, si hay luz, la apaga. Las acciones preventivas aprendidas exigen una probabilidad de 0,85.

use std::collections::HashSet;

use crate::domain::Parameter;
use crate::engine::diagnosis::{ParameterDiagnosis, Severity, Status};
use crate::engine::forecast::{Forecast, Limit};

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub actuator: &'static str,
    pub action: &'static str,
    pub duration_seconds: Option<i64>,
    pub reason: &'static str,
}

/// Umbral de las acciones preventivas: la variable saldrá de su rango en menos de esta cantidad de horas.
pub const PREVENTIVE_HOURS: f64 = 1.0;
/// Probabilidad aprendida desde la que el agente actúa de forma preventiva (más exigente que la conclusión).
pub const LEARNED_ACTION: f64 = 0.85;

/// Lo que el agente necesita de la memoria de trabajo.
#[derive(Debug, Clone, Copy, Default)]
pub struct Facts {
    pub sensor_fault: bool,
    pub raining: bool,
    pub lockout: bool,
    pub learned_needs_water: Option<f64>,
    pub learned_overheat: Option<f64>,
}

/// Predicciones de los modelos base.
#[derive(Debug, Clone, Copy, Default)]
pub struct Predictions {
    pub ventilation: Option<f64>,
    pub ph_correction: Option<f64>,
}

pub fn decide(
    diagnosis: &[ParameterDiagnosis],
    predictions: Predictions,
    facts: Facts,
    actuators: &HashSet<String>,
    resting: bool,
    forecasts: &[Forecast],
) -> Vec<Action> {
    if facts.sensor_fault {
        return Vec::new();
    }
    let find = |p: Parameter| diagnosis.iter().find(|d| d.parameter == p);
    let mut actions: Vec<Action> = Vec::new();
    let mut propose = |actuator: &'static str,
                       action: &'static str,
                       duration: Option<i64>,
                       reason: &'static str| {
        if actuators.contains(actuator) && actions.iter().all(|a| a.actuator != actuator) {
            actions.push(Action {
                actuator,
                action,
                duration_seconds: duration,
                reason,
            });
        }
    };
    let upcoming = |p: Parameter| -> Option<Limit> {
        forecasts
            .iter()
            .rfind(|f| f.parameter == p && f.hours_to_limit.is_some_and(|h| h <= PREVENTIVE_HOURS))
            .and_then(|f| f.limit)
    };
    let status = |d: Option<&ParameterDiagnosis>, s: Status| d.is_some_and(|d| d.status == s);

    let soil = find(Parameter::SoilMoisture);
    if facts.raining {
        // Llueve sobre un cultivo al aire libre: la lluvia riega.
    } else if status(soil, Status::Low) {
        let critical = soil.is_some_and(|d| d.severity == Severity::Critical);
        propose(
            "WATER_PUMP",
            "ACTIVATE",
            Some(if critical { 30 } else { 15 }),
            "The substrate is dry: automatic irrigation.",
        );
    } else if status(soil, Status::Optimal) && upcoming(Parameter::SoilMoisture) == Some(Limit::Min)
    {
        propose(
            "WATER_PUMP",
            "ACTIVATE",
            Some(10),
            "Preventive irrigation: the substrate will reach the minimum in less than 1 h.",
        );
    } else if status(soil, Status::Optimal)
        && !resting
        && facts.learned_needs_water.unwrap_or(0.0) >= LEARNED_ACTION
    {
        propose(
            "WATER_PUMP",
            "ACTIVATE",
            Some(10),
            "Preventive irrigation: learned models for this species anticipate the substrate will dry out within the next hour.",
        );
    }

    let light = find(Parameter::Brightness);
    if resting && light.is_some_and(|d| matches!(d.status, Status::Optimal | Status::High)) {
        propose(
            "UV_LIGHT",
            "DEACTIVATE",
            None,
            "It is nighttime: turning off the UV light to respect the plant's rest period.",
        );
    } else if status(light, Status::Low) {
        propose(
            "UV_LIGHT",
            "ACTIVATE",
            Some(900),
            "Insufficient light: turning on the UV light for 15 minutes.",
        );
    } else if status(light, Status::High) {
        propose(
            "UV_LIGHT",
            "DEACTIVATE",
            None,
            "Excess light: turning off the UV light.",
        );
    }

    let temperature = find(Parameter::Temperature);
    let humidity = find(Parameter::Humidity);
    let hot_or_humid = status(temperature, Status::High) || status(humidity, Status::High);
    if hot_or_humid || predictions.ventilation.unwrap_or(0.0) >= 0.7 {
        propose(
            "FAN",
            "ACTIVATE",
            Some(600),
            "High heat or humidity: ventilating for 10 minutes.",
        );
    } else if status(temperature, Status::Optimal)
        && upcoming(Parameter::Temperature) == Some(Limit::Max)
    {
        propose(
            "FAN",
            "ACTIVATE",
            Some(600),
            "Preventive ventilation: temperature will exceed the maximum in less than 1 h.",
        );
    } else if status(temperature, Status::Optimal)
        && facts.learned_overheat.unwrap_or(0.0) >= LEARNED_ACTION
    {
        propose(
            "FAN",
            "ACTIVATE",
            Some(600),
            "Preventive ventilation: learned models for this species anticipate heat within the next hour.",
        );
    } else if status(temperature, Status::Low) {
        propose(
            "FAN",
            "DEACTIVATE",
            None,
            "Low temperature: turning off ventilation.",
        );
    }

    if status(humidity, Status::Low) {
        propose(
            "HUMIDIFIER",
            "ACTIVATE",
            Some(300),
            "Dry air: running the humidifier for 5 minutes.",
        );
    }

    // Como en Python, `prediccion or 1.0`: sin predicción (o con 0 exacto) se asume que hace falta corregir.
    let ph_need = predictions
        .ph_correction
        .filter(|p| *p != 0.0)
        .unwrap_or(1.0);
    if status(find(Parameter::Ph), Status::High) && ph_need >= 0.5 {
        propose(
            "PH_DOSER",
            "ACTIVATE",
            Some(3),
            "High pH: short dose of pH Down solution.",
        );
    }

    if status(find(Parameter::Tds), Status::Low) && !facts.lockout {
        propose(
            "NUTRIENT_DOSER",
            "ACTIVATE",
            Some(3),
            "Low nutrients: short dose of nutrient solution.",
        );
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile;
    use crate::engine::diagnosis::diagnose;

    fn all() -> HashSet<String> {
        [
            "WATER_PUMP",
            "UV_LIGHT",
            "FAN",
            "HUMIDIFIER",
            "PH_DOSER",
            "NUTRIENT_DOSER",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn values(pairs: &[(Parameter, f64)]) -> crate::domain::Values {
        let mut v = [None; 6];
        for &(p, x) in pairs {
            v[p.index()] = Some(x);
        }
        v
    }

    #[test]
    fn never_turns_the_light_on_at_night() {
        let tomato = profile("TOMATO").unwrap();
        for hour in [22, 23, 0, 1, 2, 3, 4, 5] {
            let found = diagnose(
                tomato,
                &values(&[(Parameter::Brightness, 10.0)]),
                Some(hour),
            );
            let actions = decide(
                &found,
                Predictions::default(),
                Facts::default(),
                &all(),
                true,
                &[],
            );
            assert!(
                actions
                    .iter()
                    .all(|a| !(a.actuator == "UV_LIGHT" && a.action == "ACTIVATE")),
                "hora {hour}"
            );
        }
        let lit = diagnose(
            tomato,
            &values(&[(Parameter::Brightness, 1000.0)]),
            Some(23),
        );
        let actions = decide(
            &lit,
            Predictions::default(),
            Facts::default(),
            &all(),
            true,
            &[],
        );
        assert_eq!(actions[0].actuator, "UV_LIGHT");
        assert_eq!(actions[0].action, "DEACTIVATE");
    }

    #[test]
    fn learned_watering_needs_high_confidence_and_daylight() {
        let lettuce = profile("LETTUCE").unwrap();
        let found = diagnose(
            lettuce,
            &values(&[(Parameter::SoilMoisture, 65.0)]),
            Some(12),
        );
        let mut facts = Facts {
            learned_needs_water: Some(0.84),
            ..Facts::default()
        };
        assert!(decide(&found, Predictions::default(), facts, &all(), false, &[]).is_empty());
        facts.learned_needs_water = Some(0.86);
        let actions = decide(&found, Predictions::default(), facts, &all(), false, &[]);
        assert_eq!(actions[0].duration_seconds, Some(10));
        assert!(decide(&found, Predictions::default(), facts, &all(), true, &[]).is_empty());
    }

    #[test]
    fn only_uses_the_actuators_the_crop_has_and_stops_on_sensor_fault() {
        let basil = profile("BASIL").unwrap();
        let found = diagnose(
            basil,
            &values(&[(Parameter::SoilMoisture, 10.0), (Parameter::Humidity, 10.0)]),
            Some(12),
        );
        let only_pump: HashSet<String> = ["WATER_PUMP".to_string()].into();
        let actions = decide(
            &found,
            Predictions::default(),
            Facts::default(),
            &only_pump,
            false,
            &[],
        );
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].duration_seconds, Some(30));
        let faulty = Facts {
            sensor_fault: true,
            ..Facts::default()
        };
        assert!(decide(&found, Predictions::default(), faulty, &all(), false, &[]).is_empty());
    }

    #[test]
    fn rain_skips_irrigation() {
        let basil = profile("BASIL").unwrap();
        let found = diagnose(basil, &values(&[(Parameter::SoilMoisture, 10.0)]), Some(12));
        let wet = Facts {
            raining: true,
            ..Facts::default()
        };
        assert!(decide(&found, Predictions::default(), wet, &all(), false, &[]).is_empty());
    }
}

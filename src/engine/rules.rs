//! Las 21 reglas del sistema experto de SmartPot.
//!
//! La memoria de trabajo arranca con el diagnóstico por variable (estado, severidad y valor) y, si existen, con las
//! predicciones de los modelos base, los pronósticos, lo aprendido de las lecturas reales, el lugar del cultivo y el
//! clima de afuera. Las reglas de prioridad alta combinan variables; las de prioridad baja encadenan conclusiones.
//! Los textos son los mismos del servicio en Python.

use crate::domain::placement::Setting;
use crate::domain::Parameter::{self, Brightness, Humidity, Ph, SoilMoisture, Tds, Temperature};
use crate::engine::diagnosis::Status::{self, High, Low, Optimal, Rest};
use crate::engine::expert::{Memory, Rule};
use crate::engine::forecast::Limit;
use crate::format::{one_decimal, percent};
use crate::schema::Outside;

/// Probabilidad desde la que una predicción aprendida se convierte en conclusión.
pub const LEARNED_CERTAINTY: f64 = 0.7;
/// Lluvia (mm por hora) desde la que no hace falta regar un cultivo al aire libre.
pub const RAIN_MM: f64 = 0.5;
/// Diferencia (°C) entre el sensor y el clima de afuera desde la que conviene revisar el sensor.
pub const SENSOR_GAP: f64 = 8.0;

/// Índices de las reglas que otras reglas consultan (`rule:<nombre>`).
pub const FUNGAL_RISK: usize = 1;
pub const DRYING_TREND: usize = 12;
pub const HEAT_BUILDING: usize = 13;

fn is(m: &Memory, parameter: Parameter, status: Status) -> bool {
    m.status(parameter) == Some(status)
}

fn warm(m: &Memory) -> bool {
    matches!(m.status(Temperature), Some(Optimal | High))
}

/// Horas hasta que el pronóstico cruce el límite indicado, si ocurre dentro de 3 h.
fn approaching(m: &Memory, parameter: Parameter, limit: Limit) -> Option<f64> {
    let fact = m.forecasts[parameter.index()]?;
    match (fact.limit, fact.hours) {
        (Some(found), Some(hours)) if found == limit && hours <= 3.0 => Some(hours),
        _ => None,
    }
}

/// Clima de afuera, solo si el cultivo está al aire libre.
fn outdoor(m: &Memory) -> Option<&Outside> {
    if m.setting != Some(Setting::Outdoor) {
        return None;
    }
    m.outside.as_ref()
}

fn raining(m: &Memory) -> bool {
    outdoor(m).is_some_and(|o| o.precipitation >= RAIN_MM)
}

fn sensor_gap(m: &Memory) -> Option<f64> {
    let outside = outdoor(m)?;
    let measured = m.value[Temperature.index()]?;
    let gap = measured - outside.temperature;
    (gap.abs() >= SENSOR_GAP).then_some(gap)
}

fn nothing(_: &mut Memory) {}

pub static RULES: [Rule; 21] = [
    Rule {
        name: "heat_stress",
        title: "Estrés térmico",
        certainty: 0.9,
        salience: 20,
        condition: |m| is(m, Temperature, High) && is(m, Humidity, Low),
        message: |_| {
            "Calor con aire seco: la planta pierde agua más rápido de lo que la absorbe.".into()
        },
        conclude: |m| m.stress = Some("heat"),
    },
    Rule {
        name: "fungal_risk",
        title: "Riesgo de hongos",
        certainty: 0.8,
        salience: 20,
        condition: |m| is(m, Humidity, High) && warm(m),
        message: |_| {
            "Humedad alta con temperatura templada: condiciones favorables para hongos y moho."
                .into()
        },
        conclude: |m| m.risk = Some("fungal"),
    },
    Rule {
        name: "nutrient_lockout",
        title: "Bloqueo de nutrientes",
        certainty: 0.85,
        salience: 20,
        condition: |m| {
            matches!(m.status(Ph), Some(Low | High))
                && matches!(m.status(Tds), Some(Optimal | High))
        },
        message: |_| {
            "Hay nutrientes en la solución, pero el pH fuera de rango impide absorberlos. \
             Corrige el pH antes de agregar más nutrientes."
                .into()
        },
        conclude: |m| m.lockout = true,
    },
    Rule {
        name: "root_rot_risk",
        title: "Riesgo de pudrición de raíz",
        certainty: 0.75,
        salience: 20,
        condition: |m| is(m, SoilMoisture, High) && is(m, Temperature, High),
        message: |_| {
            "Sustrato encharcado y caliente: las raíces pierden oxígeno y aparecen patógenos."
                .into()
        },
        conclude: |m| m.risk = Some("root_rot"),
    },
    Rule {
        name: "drought",
        title: "Sequía",
        certainty: 0.9,
        salience: 15,
        condition: |m| m.critical(SoilMoisture) && is(m, SoilMoisture, Low),
        message: |_| "El sustrato está muy seco: riega pronto para evitar marchitez.".into(),
        conclude: |m| m.stress = Some("water"),
    },
    Rule {
        name: "etiolation",
        title: "Crecimiento ahilado",
        certainty: 0.7,
        salience: 15,
        condition: |m| is(m, Brightness, Low) && warm(m),
        message: |_| {
            "Poca luz con temperatura cálida: la planta estira sus tallos buscando luz y se debilita.".into()
        },
        conclude: nothing,
    },
    Rule {
        name: "cold_stress",
        title: "Estrés por frío",
        certainty: 0.9,
        salience: 15,
        condition: |m| m.critical(Temperature) && is(m, Temperature, Low),
        message: |_| {
            "La temperatura está muy baja para la especie: el crecimiento se detiene.".into()
        },
        conclude: |m| m.stress = Some("cold"),
    },
    Rule {
        name: "salt_stress",
        title: "Exceso de sales",
        certainty: 0.85,
        salience: 15,
        condition: |m| m.critical(Tds) && is(m, Tds, High),
        message: |_| {
            "La concentración de sales es tan alta que puede quemar las raíces. Diluye la solución."
                .into()
        },
        conclude: nothing,
    },
    Rule {
        name: "ventilation_model",
        title: "El modelo recomienda ventilar",
        certainty: 0.7,
        salience: 10,
        condition: |m| m.ventilation.unwrap_or(0.0) >= 0.7 && !m.has_fired(FUNGAL_RISK),
        message: |m| {
            format!(
                "La regresión logística estima {} de necesidad de ventilación por la combinación de temperatura y humedad.",
                percent(m.ventilation.unwrap_or(0.0))
            )
        },
        conclude: nothing,
    },
    Rule {
        name: "sensor_fault",
        title: "Posible falla de sensor",
        certainty: 0.6,
        salience: 5,
        condition: |m| m.anomaly.unwrap_or(0.0) >= 0.8 && m.critical_count() >= 2,
        message: |_| {
            "La lectura es atípica frente al historial y varias variables están en valores extremos. \
             Revisa la conexión y calibración de los sensores antes de actuar."
                .into()
        },
        conclude: |m| m.sensor_fault = true,
    },
    Rule {
        name: "critical_state",
        title: "Estado crítico",
        certainty: 0.95,
        salience: 0,
        condition: |m| m.critical_count() >= 2 && !m.sensor_fault,
        message: |m| {
            format!(
                "{} variables están en nivel crítico: el cultivo necesita atención inmediata.",
                m.critical_count()
            )
        },
        conclude: nothing,
    },
    Rule {
        name: "compound_stress",
        title: "Estrés combinado",
        certainty: 0.8,
        salience: -5,
        condition: |m| m.stress.is_some() && (m.risk.is_some() || m.lockout),
        message: |_| {
            "Varios factores de estrés actúan a la vez; corrige primero el más severo y observa la respuesta.".into()
        },
        conclude: nothing,
    },
    Rule {
        name: "drying_trend",
        title: "Secado acelerado",
        certainty: 0.75,
        salience: 12,
        condition: |m| {
            is(m, SoilMoisture, Optimal) && approaching(m, SoilMoisture, Limit::Min).is_some()
        },
        message: |m| {
            format!(
                "El sustrato se seca rápido: llegará al mínimo en {} h si sigue la tendencia. Conviene regar pronto.",
                one_decimal(approaching(m, SoilMoisture, Limit::Min).unwrap_or(0.0))
            )
        },
        conclude: |m| m.trend = Some("drying"),
    },
    Rule {
        name: "heat_building",
        title: "Calor en aumento",
        certainty: 0.7,
        salience: 12,
        condition: |m| {
            is(m, Temperature, Optimal) && approaching(m, Temperature, Limit::Max).is_some()
        },
        message: |m| {
            format!(
                "La temperatura sube y pasará el máximo en {} h: ventila o aleja el cultivo del sol antes de que llegue.",
                one_decimal(approaching(m, Temperature, Limit::Max).unwrap_or(0.0))
            )
        },
        conclude: |m| m.trend = Some("heating"),
    },
    Rule {
        name: "learned_drying",
        title: "Riego probable en la próxima hora",
        certainty: 0.7,
        salience: 11,
        condition: |m| {
            is(m, SoilMoisture, Optimal)
                && m.learned_needs_water.unwrap_or(0.0) >= LEARNED_CERTAINTY
                && !m.has_fired(DRYING_TREND)
        },
        message: |m| {
            format!(
                "Lo aprendido de los cultivos de esta especie da {} de probabilidad de que el sustrato baje del mínimo en la próxima hora.",
                percent(m.learned_needs_water.unwrap_or(0.0))
            )
        },
        conclude: |m| m.trend = Some("drying"),
    },
    Rule {
        name: "learned_heat",
        title: "Calor probable en la próxima hora",
        certainty: 0.65,
        salience: 11,
        condition: |m| {
            is(m, Temperature, Optimal)
                && m.learned_overheat.unwrap_or(0.0) >= LEARNED_CERTAINTY
                && !m.has_fired(HEAT_BUILDING)
        },
        message: |m| {
            format!(
                "Lo aprendido de los cultivos de esta especie da {} de probabilidad de que la temperatura pase del máximo en la próxima hora.",
                percent(m.learned_overheat.unwrap_or(0.0))
            )
        },
        conclude: |m| m.trend = Some("heating"),
    },
    Rule {
        name: "unusual_pattern",
        title: "Combinación poco habitual",
        certainty: 0.6,
        salience: 4,
        condition: |m| m.learned_unusual.is_some() && !m.sensor_fault,
        message: |_| {
            "Esta combinación de valores casi nunca se ha visto en los cultivos de la especie. \
             Si nada cambió en el cultivo, revisa los sensores."
                .into()
        },
        conclude: nothing,
    },
    Rule {
        name: "rain_outside",
        title: "Está lloviendo",
        certainty: 0.9,
        salience: 8,
        condition: raining,
        message: |m| {
            format!(
                "Llueve donde está el cultivo ({} mm): la lluvia riega por ti. Si el sustrato se encharca, cúbrelo o revisa el drenaje.",
                one_decimal(outdoor(m).map_or(0.0, |o| o.precipitation))
            )
        },
        conclude: |m| m.raining = true,
    },
    Rule {
        name: "sensor_vs_outside",
        title: "El sensor no coincide con el clima",
        certainty: 0.6,
        salience: 6,
        condition: |m| sensor_gap(m).is_some(),
        message: |m| {
            let outside_temperature = outdoor(m).map_or(0.0, |o| o.temperature);
            let radiation = outdoor(m).map_or(0.0, |o| o.radiation);
            let advice = if sensor_gap(m).unwrap_or(0.0) > 0.0 && radiation >= 400.0 {
                "Si le da el sol directo al sensor, dale sombra para que mida el aire."
            } else {
                "Revisa el sensor o confirma que el cultivo sí está al aire libre."
            };
            format!(
                "El sensor marca {} °C y afuera hay {} °C. {advice}",
                one_decimal(m.value[Temperature.index()].unwrap_or(0.0)),
                one_decimal(outside_temperature)
            )
        },
        conclude: nothing,
    },
    Rule {
        name: "night_rest",
        title: "Descanso nocturno",
        certainty: 1.0,
        salience: -10,
        condition: |m| is(m, Brightness, Rest),
        message: |_| {
            "La planta está en su periodo de oscuridad: no hace falta encender la luz ultravioleta hasta la mañana."
                .into()
        },
        conclude: nothing,
    },
    Rule {
        name: "ideal_conditions",
        title: "Condiciones ideales",
        certainty: 1.0,
        salience: -10,
        condition: |m| {
            let measured: Vec<Status> = m.status.iter().flatten().copied().collect();
            !measured.is_empty() && measured.iter().all(|s| matches!(s, Optimal | Rest))
        },
        message: |_| "Todas las variables medidas están en su rango ideal. ¡Buen trabajo!".into(),
        conclude: nothing,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::diagnosis::Severity;
    use crate::engine::expert::{ForecastFact, InferenceEngine};

    fn memory(states: &[(Parameter, Status, Severity, f64)]) -> Memory {
        let mut m = Memory::default();
        for &(p, s, sev, v) in states {
            m.status[p.index()] = Some(s);
            m.severity[p.index()] = Some(sev);
            m.value[p.index()] = Some(v);
        }
        m
    }

    fn fired(m: &mut Memory) -> Vec<&'static str> {
        InferenceEngine::new(&RULES)
            .run(m)
            .into_iter()
            .map(|f| f.rule)
            .collect()
    }

    #[test]
    fn there_are_twenty_one_unique_rules() {
        let mut names: Vec<&str> = RULES.iter().map(|r| r.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 21);
        assert_eq!(RULES[FUNGAL_RISK].name, "fungal_risk");
        assert_eq!(RULES[DRYING_TREND].name, "drying_trend");
        assert_eq!(RULES[HEAT_BUILDING].name, "heat_building");
    }

    #[test]
    fn chains_stress_with_a_risk() {
        let mut m = memory(&[
            (Temperature, High, Severity::Critical, 35.0),
            (Humidity, Low, Severity::Warning, 30.0),
            (Ph, High, Severity::Warning, 7.0),
            (Tds, Optimal, Severity::Ok, 700.0),
        ]);
        assert_eq!(
            fired(&mut m),
            ["heat_stress", "nutrient_lockout", "compound_stress"]
        );
    }

    #[test]
    fn two_critical_variables_without_anomaly_are_a_critical_state() {
        let mut m = memory(&[
            (Temperature, Low, Severity::Critical, 2.0),
            (SoilMoisture, Low, Severity::Critical, 10.0),
        ]);
        let names = fired(&mut m);
        assert!(names.contains(&"critical_state"));
        assert!(!names.contains(&"compound_stress"));
        m = memory(&[
            (Temperature, Low, Severity::Critical, 2.0),
            (SoilMoisture, Low, Severity::Critical, 10.0),
        ]);
        m.anomaly = Some(0.95);
        let names = fired(&mut m);
        assert!(names.contains(&"sensor_fault") && !names.contains(&"critical_state"));
    }

    #[test]
    fn a_forecast_turns_into_a_drying_trend() {
        let mut m = memory(&[(SoilMoisture, Optimal, Severity::Ok, 62.0)]);
        m.forecasts[SoilMoisture.index()] = Some(ForecastFact {
            limit: Some(Limit::Min),
            hours: Some(1.4),
        });
        m.learned_needs_water = Some(0.9);
        let trace = InferenceEngine::new(&RULES).run(&mut m);
        assert_eq!(trace[0].rule, "drying_trend");
        assert!(trace[0].message.contains("1.4 h"));
        assert!(trace.iter().all(|f| f.rule != "learned_drying"));
    }

    #[test]
    fn ideal_conditions_need_at_least_one_measure() {
        let mut empty = Memory::default();
        assert!(fired(&mut empty).is_empty());
        let mut night = memory(&[
            (Brightness, Rest, Severity::Ok, 0.0),
            (Ph, Optimal, Severity::Ok, 6.0),
        ]);
        assert_eq!(fired(&mut night), ["night_rest", "ideal_conditions"]);
    }
}

//! Orquesta la evaluación: diagnóstico → pronóstico → modelos base y aprendidos → sistema experto → lógica difusa →
//! agente → consejo de lugar.

use std::collections::HashSet;
use std::sync::OnceLock;

use crate::domain::CropProfile;
use crate::engine::agent::{self, Facts, Predictions};
use crate::engine::diagnosis::{diagnose, is_rest_hour, ParameterDiagnosis, Status};
use crate::engine::expert::{ForecastFact, InferenceEngine, Memory};
use crate::engine::forecast::forecast;
use crate::engine::fuzzy::health_index;
use crate::engine::models::normalize;
use crate::engine::placement::advise;
use crate::engine::rules::RULES;
use crate::format::{no_decimals, py_round};
use crate::learning::registry::LiveReading;
use crate::learning::Counts;
use crate::schema::response::{
    Action, Conclusion, Diagnosis, Forecast, InsightResponse, Prediction,
};
use crate::schema::{InsightRequest, Micros};
use crate::state::AppState;

fn engine() -> &'static InferenceEngine {
    static ENGINE: OnceLock<InferenceEngine> = OnceLock::new();
    ENGINE.get_or_init(|| InferenceEngine::new(&RULES))
}

fn seconds(micros: Micros) -> f64 {
    micros as f64 / 1e6
}

/// Evalúa una lectura. `now` (segundos epoch) se usa cuando la lectura no trae su hora.
pub fn analyze(state: &AppState, request: &InsightRequest, now: f64) -> InsightResponse {
    let profile = request.profile;
    let values = &request.measures.values;
    let diagnosis = diagnose(profile, values, request.local_hour);

    let timed: Vec<(Micros, crate::domain::Values)> = request
        .history
        .iter()
        .filter_map(|item| item.measured_at.map(|at| (at, item.values)))
        .collect();
    let forecasts = forecast(profile, &timed);

    let counts = match &state.trainer {
        Some(trainer) => trainer.counts(profile.kind),
        None => Counts::default(),
    };
    let reading = LiveReading {
        values,
        hour: request.local_hour,
        moment: request.measures.measured_at.map_or(now, seconds),
        history: timed.iter().map(|(at, v)| (seconds(*at), *v)).collect(),
    };
    let (learning, learned) =
        state
            .learned
            .infer(profile, &reading, counts, state.settings.min_samples);

    let positions = normalize(profile, values);
    let history: Vec<crate::domain::Values> = request
        .history
        .iter()
        .map(|item| normalize(profile, &item.values))
        .collect();
    let ventilation = state.base.ventilation_probability(&positions);
    let ph_correction = state.base.ph_correction_probability(&positions);
    let anomaly = positions
        .iter()
        .any(Option::is_some)
        .then(|| state.base.anomaly_probability(&positions, &history));

    let mut memory = Memory::default();
    for item in &diagnosis {
        let i = item.parameter.index();
        memory.status[i] = Some(item.status);
        memory.value[i] = Some(item.value);
        memory.severity[i] = Some(item.severity);
    }
    memory.ventilation = ventilation;
    memory.ph_correction = ph_correction;
    memory.anomaly = anomaly;
    for item in &forecasts {
        memory.forecasts[item.parameter.index()] = Some(ForecastFact {
            limit: item.limit,
            hours: item.hours_to_limit,
        });
    }
    let place = request.placement.unwrap_or_default();
    memory.setting = place.setting;
    memory.exposure = place.exposure;
    memory.outside = request.weather.clone();
    memory.learned_needs_water = learned.needs_water;
    memory.learned_overheat = learned.overheat;
    memory.learned_unusual = learned.unusual;
    let firings = engine().run(&mut memory);

    let deviations: Vec<_> = diagnosis
        .iter()
        .map(|d| (d.parameter, d.deviation))
        .collect();
    let health = health_index(&deviations);
    let actuators: HashSet<String> = request.actuators.iter().map(|a| a.to_uppercase()).collect();
    let facts = Facts {
        sensor_fault: memory.sensor_fault,
        raining: memory.raining,
        lockout: memory.lockout,
        learned_needs_water: memory.learned_needs_water,
        learned_overheat: memory.learned_overheat,
    };
    let actions = agent::decide(
        &diagnosis,
        Predictions {
            ventilation,
            ph_correction,
        },
        facts,
        &actuators,
        is_rest_hour(request.local_hour),
        &forecasts,
    );
    let sunny = request
        .weather
        .as_ref()
        .is_some_and(|w| w.is_day && w.radiation >= 300.0);
    let advice = advise(profile, place.setting, place.exposure, &diagnosis, sunny);
    let summary = summary(
        profile,
        health.index,
        health.label,
        &diagnosis,
        actions.len(),
        advice.level == "MOVE",
    );

    let mut predictions = Vec::new();
    for (name, value, label, model) in [
        (
            "ventilation",
            ventilation,
            "Necesidad de ventilación",
            "Regresión logística",
        ),
        (
            "ph_correction",
            ph_correction,
            "Necesidad de corregir el pH",
            "Red neuronal (MLP)",
        ),
        ("anomaly", anomaly, "Lectura atípica", "Isolation Forest"),
    ] {
        if let Some(value) = value {
            predictions.push(Prediction {
                name,
                label,
                probability: py_round(value, 3),
                model,
            });
        }
    }

    InsightResponse {
        crop_type: profile.kind,
        health,
        diagnosis: diagnosis
            .iter()
            .map(|d| Diagnosis {
                parameter: d.parameter.name(),
                value: d.value,
                status: d.status.name(),
                severity: d.severity.name(),
                message: d.message.clone(),
                recommendation: d.recommendation,
            })
            .collect(),
        conclusions: firings
            .into_iter()
            .map(|f| Conclusion {
                rule: f.rule,
                title: f.title,
                message: f.message,
                certainty: f.certainty,
            })
            .collect(),
        predictions,
        actions: actions
            .into_iter()
            .map(|a| Action {
                actuator: a.actuator,
                action: a.action,
                duration_seconds: a.duration_seconds,
                reason: a.reason,
            })
            .collect(),
        forecasts: forecasts
            .into_iter()
            .map(|f| Forecast {
                parameter: f.parameter.name(),
                current: f.current,
                slope_per_hour: f.slope_per_hour,
                expected_in_3h: f.expected_in_3h,
                trend: f.trend.name(),
                hours_to_limit: f.hours_to_limit,
                limit: f.limit.map(|l| l.name()),
                confidence: f.confidence,
                message: f.message,
            })
            .collect(),
        learning: Some(learning),
        placement: Some(advice),
        summary,
    }
}

fn summary(
    profile: &CropProfile,
    index: f64,
    label: &str,
    diagnosis: &[ParameterDiagnosis],
    action_count: usize,
    move_it: bool,
) -> String {
    let mut issues: Vec<&ParameterDiagnosis> = diagnosis
        .iter()
        .filter(|d| !matches!(d.status, Status::Optimal | Status::Rest))
        .collect();
    issues.sort_by(|a, b| b.deviation.total_cmp(&a.deviation));
    let mut text = format!(
        "Tu {} está en estado «{label}» ({}/100).",
        profile.name.to_lowercase(),
        no_decimals(index)
    );
    if issues.is_empty() {
        return text + " Todas las variables medidas están en su rango ideal.";
    }
    let names: Vec<&str> = issues
        .iter()
        .take(2)
        .map(|d| profile.range(d.parameter).label)
        .collect();
    text.push_str(&format!(" Revisa {}.", names.join(" y ")));
    if action_count > 0 {
        let noun = if action_count == 1 {
            "acción"
        } else {
            "acciones"
        };
        text.push_str(&format!(" El asistente propone {action_count} {noun}."));
    }
    if move_it {
        text.push_str(" También conviene cambiar el cultivo de lugar.");
    }
    text
}

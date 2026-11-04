//! Análisis de todos los cultivos de una cuenta a la vez.
//!
//! - Ranking por índice de salud (lógica difusa de cada cultivo).
//! - Problemas compartidos: la misma variable fuera de rango en la mitad o más de los cultivos suele ser un problema
//!   del entorno (la habitación, el agua, la solución nutritiva) y no de un cultivo.
//! - Grupos por condiciones similares: K-Means determinista sobre las variables normalizadas respecto a cada perfil,
//!   así una lechuga y un tomate son comparables. La cantidad de grupos se elige por silueta.
//! - Acciones agregadas: las que el agente propone para cada cultivo, reunidas por actuador para aplicarlas en bloque.

use std::collections::HashSet;

use crate::domain::Parameter;
use crate::engine::agent::{self, Facts, Predictions};
use crate::engine::diagnosis::{diagnose, is_rest_hour, ParameterDiagnosis, Status};
use crate::engine::fuzzy::health_index;
use crate::engine::models::normalize;
use crate::format::{capitalize, no_decimals, py_round};
use crate::ml::kmeans;
use crate::numeric::{mean, np_round};
use crate::schema::response::{
    CropGroup, FleetAction, FleetCropResult, FleetResponse, Health, SharedIssue,
};
use crate::schema::FleetRequest;

pub const GROUP_PARAMETERS: [Parameter; 5] = [
    Parameter::Temperature,
    Parameter::Humidity,
    Parameter::Ph,
    Parameter::Tds,
    Parameter::SoilMoisture,
];
pub const SHARED_SHARE: f64 = 0.5;
const SEED: u32 = 42;

/// Cómo se nombra cada variable fuera de rango (por debajo, por encima).
pub fn phrase(parameter: Parameter, status: Status) -> &'static str {
    let (low, high) = match parameter {
        Parameter::Temperature => ("low temperature", "high temperature"),
        Parameter::Humidity => ("dry air", "very humid air"),
        Parameter::Brightness => ("low light", "excess light"),
        Parameter::Ph => ("low pH", "high pH"),
        Parameter::Tds => ("low nutrients", "excess nutrients"),
        Parameter::SoilMoisture => ("dry substrate", "waterlogged substrate"),
    };
    if status == Status::Low {
        low
    } else {
        high
    }
}

struct CropResult {
    id: String,
    name: String,
    crop_type: &'static str,
    health: Option<Health>,
    diagnosis: Vec<ParameterDiagnosis>,
    rank: Option<usize>,
}

pub fn analyze(request: &FleetRequest) -> FleetResponse {
    let resting = is_rest_hour(request.local_hour);
    let mut results: Vec<CropResult> = request
        .crops
        .iter()
        .map(|crop| {
            let diagnosis = match &crop.measures {
                Some(m) => diagnose(crop.profile, &m.values, request.local_hour),
                None => Vec::new(),
            };
            let health = (!diagnosis.is_empty()).then(|| {
                health_index(
                    &diagnosis
                        .iter()
                        .map(|d| (d.parameter, d.deviation))
                        .collect::<Vec<_>>(),
                )
            });
            CropResult {
                id: crop.id.clone(),
                name: crop.name.clone(),
                crop_type: crop.profile.kind,
                health,
                diagnosis,
                rank: None,
            }
        })
        .collect();

    let mut measured: Vec<usize> = (0..results.len())
        .filter(|&i| results[i].health.is_some())
        .collect();
    measured.sort_by(|&a, &b| index_of(&results[a]).total_cmp(&index_of(&results[b])));
    for (rank, &i) in measured.iter().enumerate() {
        results[i].rank = Some(rank + 1);
    }
    let average = (!measured.is_empty()).then(|| {
        let indexes: Vec<f64> = measured.iter().map(|&i| index_of(&results[i])).collect();
        py_round(mean(&indexes), 1)
    });

    let shared = shared_issues(&results, &measured);
    let groups = groups(request, &results);
    let actions = fleet_actions(request, &results, resting);
    let summary = summary(average, &results, &measured, &shared, &actions);

    FleetResponse {
        average_health: average,
        crops: results
            .into_iter()
            .map(|r| FleetCropResult {
                issues: r
                    .diagnosis
                    .iter()
                    .filter(|d| d.status.is_out())
                    .map(|d| capitalize(phrase(d.parameter, d.status)))
                    .collect(),
                id: r.id,
                name: r.name,
                crop_type: r.crop_type,
                rank: r.rank,
                health: r.health,
            })
            .collect(),
        shared_issues: shared,
        groups,
        actions,
        summary,
    }
}

fn index_of(result: &CropResult) -> f64 {
    result.health.as_ref().map_or(100.0, |h| h.index)
}

fn shared_issues(results: &[CropResult], measured: &[usize]) -> Vec<SharedIssue> {
    if measured.len() < 2 {
        return Vec::new();
    }
    let mut issues = Vec::new();
    for parameter in Parameter::ALL {
        for status in [Status::Low, Status::High] {
            let ids: Vec<String> = measured
                .iter()
                .map(|&i| &results[i])
                .filter(|r| {
                    r.diagnosis
                        .iter()
                        .any(|d| d.parameter == parameter && d.status == status)
                })
                .map(|r| r.id.clone())
                .collect();
            let share = ids.len() as f64 / measured.len() as f64;
            if ids.len() >= 2 && share >= SHARED_SHARE {
                let text = phrase(parameter, status);
                let message = format!(
                    "{} in {} of {} crops: likely an environmental issue rather than an individual crop.",
                    capitalize(text),
                    ids.len(),
                    measured.len()
                );
                issues.push(SharedIssue {
                    parameter: parameter.name(),
                    status: status.name(),
                    crop_ids: ids,
                    share: py_round(share, 2),
                    message,
                });
            }
        }
    }
    issues.sort_by(|a, b| b.share.total_cmp(&a.share));
    issues
}

fn describe_group(center: &[f64]) -> (String, String) {
    let mut traits = Vec::new();
    for (value, parameter) in center.iter().zip(GROUP_PARAMETERS) {
        if *value < -0.15 {
            traits.push(phrase(parameter, Status::Low));
        } else if *value > 1.15 {
            traits.push(phrase(parameter, Status::High));
        }
    }
    if traits.is_empty() {
        return (
            "Within range".into(),
            "Group variables are, on average, within their optimal range.".into(),
        );
    }
    let label = traits
        .iter()
        .take(2)
        .copied()
        .collect::<Vec<_>>()
        .join(" and ");
    (
        capitalize(&label),
        format!("On average: {}.", traits.join(", ")),
    )
}

fn groups(request: &FleetRequest, results: &[CropResult]) -> Vec<CropGroup> {
    let crops: Vec<_> = request
        .crops
        .iter()
        .filter(|c| c.measures.is_some())
        .collect();
    if crops.len() < 3 {
        return Vec::new();
    }
    let mut data = Vec::with_capacity(crops.len());
    let mut ids = Vec::with_capacity(crops.len());
    for crop in &crops {
        let values = crop
            .measures
            .as_ref()
            .map(|m| m.values)
            .unwrap_or([None; 6]);
        let positions = normalize(crop.profile, &values);
        data.push(
            GROUP_PARAMETERS
                .iter()
                .map(|p| positions[p.index()].unwrap_or(0.5).clamp(-2.0, 3.0))
                .collect::<Vec<f64>>(),
        );
        ids.push(crop.id.clone());
    }
    let mut rounded: Vec<Vec<u64>> = data
        .iter()
        // `np.unique` trata -0.0 y 0.0 como el mismo valor.
        .map(|row| {
            row.iter()
                .map(|v| (np_round(*v, 2) + 0.0).to_bits())
                .collect()
        })
        .collect();
    rounded.sort();
    rounded.dedup();
    let distinct = rounded.len();
    if distinct < 2 {
        return vec![CropGroup {
            label: "Similar conditions".into(),
            crop_ids: ids,
            description: "All crops are under similar conditions.".into(),
        }];
    }

    let mut best: Option<(f64, kmeans::Clustering)> = None;
    let upper = 4.min(distinct - 1).min(ids.len() - 1);
    for k in 2..=upper {
        let model = kmeans::fit(&data, k, 10, SEED);
        let score = kmeans::silhouette(&data, &model.labels);
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, model));
        }
    }
    let model = best
        .map(|(_, m)| m)
        .unwrap_or_else(|| kmeans::fit(&data, 2, 10, SEED));

    let mut merged: Vec<CropGroup> = Vec::new();
    for (cluster, center) in model.centers.iter().enumerate() {
        let members: Vec<String> = model
            .labels
            .iter()
            .enumerate()
            .filter(|(_, l)| **l == cluster)
            .map(|(i, _)| ids[i].clone())
            .collect();
        let (label, description) = describe_group(center);
        if let Some(existing) = merged.iter_mut().find(|g| g.label == label) {
            existing.crop_ids.extend(members);
            existing.description = description;
        } else {
            merged.push(CropGroup {
                label,
                crop_ids: members,
                description,
            });
        }
    }
    let health = |id: &str| {
        results
            .iter()
            .find(|r| r.id == id)
            .and_then(|r| r.health.as_ref())
            .map_or(100.0, |h| h.index)
    };
    let mut keyed: Vec<(f64, CropGroup)> = merged
        .into_iter()
        .map(|g| {
            let values: Vec<f64> = g.crop_ids.iter().map(|id| health(id)).collect();
            (mean(&values), g)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    keyed.into_iter().map(|(_, g)| g).collect()
}

fn fleet_actions(
    request: &FleetRequest,
    results: &[CropResult],
    resting: bool,
) -> Vec<FleetAction> {
    let mut merged: Vec<FleetAction> = Vec::new();
    for (crop, result) in request.crops.iter().zip(results) {
        if result.diagnosis.is_empty() {
            continue;
        }
        let actuators: HashSet<String> = crop.actuators.iter().map(|a| a.to_uppercase()).collect();
        for action in agent::decide(
            &result.diagnosis,
            Predictions::default(),
            Facts::default(),
            &actuators,
            resting,
            &[],
        ) {
            let entry = match merged
                .iter_mut()
                .position(|e| e.actuator == action.actuator && e.action == action.action)
            {
                Some(i) => &mut merged[i],
                None => {
                    merged.push(FleetAction {
                        actuator: action.actuator,
                        action: action.action,
                        duration_seconds: action.duration_seconds,
                        crop_ids: Vec::new(),
                        reason: action.reason,
                    });
                    merged.last_mut().expect("just added")
                }
            };
            entry.crop_ids.push(crop.id.clone());
            if let Some(duration) = action.duration_seconds.filter(|d| *d != 0) {
                if entry.duration_seconds.unwrap_or(0) < duration {
                    entry.duration_seconds = Some(duration);
                }
            }
        }
    }
    merged.sort_by_key(|a| std::cmp::Reverse(a.crop_ids.len()));
    merged
}

fn summary(
    average: Option<f64>,
    results: &[CropResult],
    measured: &[usize],
    shared: &[SharedIssue],
    actions: &[FleetAction],
) -> String {
    let Some(average) = average else {
        return "There are no readings yet to compare your crops.".into();
    };
    let mut text = format!(
        "Your {} crops with readings average {}/100 health.",
        measured.len(),
        no_decimals(average)
    );
    let worst = &results[measured[0]];
    let worst_index = index_of(worst);
    if worst_index < 70.0 {
        text.push_str(&format!(
            " The one needing the most attention is \"{}\" ({}/100).",
            worst.name,
            no_decimals(worst_index)
        ));
    }
    if !shared.is_empty() {
        let (verb, noun) = if shared.len() == 1 {
            ("is", "shared environmental issue")
        } else {
            ("are", "shared environmental issues")
        };
        text.push_str(&format!(" There {verb} {} {noun}.", shared.len()));
    }
    if !actions.is_empty() {
        let noun = if actions.len() == 1 {
            "bulk action"
        } else {
            "bulk actions"
        };
        text.push_str(&format!(
            " The assistant suggests {} {noun}.",
            actions.len()
        ));
    }
    text
}

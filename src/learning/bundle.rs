//! Modelos aprendidos de una especie, tal como los exporta SmartPot-Trainer (`{ESPECIE}_inference.json`).
//!
//! El archivo trae las tres tareas supervisadas (¿se secará?, ¿habrá calor?, humedad en 1 h) con su modelo campeón,
//! los estados de operación (K-Means), el detector de atípicos (Isolation Forest), los adaptadores LoRA y casos de
//! control calculados por scikit-learn. Al cargarlo se evalúan esos casos: un modelo que no da lo mismo que en Python
//! queda deshabilitado (falla cerrada) en vez de responder distinto.

use std::collections::HashMap;

use serde::Deserialize;

use crate::format::py_round;
use crate::learning::features::FEATURES;
use crate::ml::lora::Adapter;
use crate::ml::{IsolationForest, ModelError, Pipeline, Scaler};

pub const SCHEMA: &str = "smartpot.learned-models/1";
pub const TASKS: [&str; 3] = ["needs_water", "overheat", "moisture_1h"];

#[derive(Debug, Clone, Deserialize)]
pub struct Card {
    pub task: String,
    pub label: String,
    pub status: String,
    pub metric: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub holdout: Option<f64>,
    #[serde(default)]
    pub estimator: Option<Pipeline>,
}

impl Card {
    pub fn usable(&self) -> bool {
        self.status == "TRAINED" && self.estimator.is_some()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Cluster {
    pub id: usize,
    pub label: String,
    pub description: String,
    pub share: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct States {
    pub scaler: Scaler,
    pub centers: Vec<Vec<f64>>,
    /// Grupo de K-Means → estado (varios grupos con el mismo nombre se funden en un estado).
    pub mapping: Vec<(usize, usize)>,
    pub clusters: Vec<Cluster>,
}

impl States {
    fn validate(&self) -> Result<(), ModelError> {
        let dims = crate::domain::Parameter::ALL.len();
        if self.scaler.mean.len() != dims
            || self.scaler.scale.len() != dims
            || self.scaler.scale.contains(&0.0)
        {
            return Err(ModelError("estados con escalado no válido".into()));
        }
        if self.centers.is_empty()
            || self
                .centers
                .iter()
                .any(|c| c.len() != dims || c.iter().any(|v| !v.is_finite()))
        {
            return Err(ModelError("estados con centros no válidos".into()));
        }
        for raw in 0..self.centers.len() {
            let target = self.target(raw);
            if !self.clusters.iter().any(|c| c.id == target) {
                return Err(ModelError("estados con un grupo sin descripción".into()));
            }
        }
        Ok(())
    }

    fn target(&self, raw: usize) -> usize {
        self.mapping
            .iter()
            .find(|(r, _)| *r == raw)
            .map_or(raw, |(_, id)| *id)
    }

    /// Estado de operación de unas posiciones: el centro más cercano tras el escalado.
    pub fn assign(&self, positions: &[f64]) -> Option<&Cluster> {
        let scaled = self.scaler.transform(positions);
        let mut best = (0usize, f64::INFINITY);
        for (j, center) in self.centers.iter().enumerate() {
            let mut distance = 0.0;
            for (a, b) in scaled.iter().zip(center) {
                distance += (a - b) * (a - b);
            }
            if distance < best.1 {
                best = (j, distance);
            }
        }
        let target = self.target(best.0);
        self.clusters.iter().find(|c| c.id == target)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Anomaly {
    pub samples: u64,
    pub forest: IsolationForest,
}

impl Anomaly {
    /// Probabilidad de atípico (redondeada como en Python) y si el bosque la marca como atípica.
    pub fn score(&self, positions: &[f64]) -> (f64, bool) {
        let decision = self.forest.decision(positions);
        (
            py_round(1.0 / (1.0 + (25.0 * decision).exp()), 3),
            decision < 0.0,
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Check {
    pub x: Vec<f64>,
    #[serde(default)]
    pub needs_water: Option<f64>,
    #[serde(default)]
    pub overheat: Option<f64>,
    #[serde(default)]
    pub moisture_1h: Option<f64>,
    #[serde(default)]
    pub state: Option<usize>,
    #[serde(default)]
    pub anomaly: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bundle {
    pub schema: String,
    pub crop_type: String,
    #[serde(default)]
    pub sklearn: Option<String>,
    #[serde(default)]
    pub trained_at: Option<f64>,
    #[serde(default)]
    pub readings_at_training: i64,
    #[serde(default)]
    pub min_samples: Option<i64>,
    pub features: usize,
    #[serde(default)]
    pub cards: Vec<Card>,
    #[serde(default)]
    pub states: Option<States>,
    #[serde(default)]
    pub anomaly: Option<Anomaly>,
    #[serde(default)]
    pub lora: HashMap<String, Adapter>,
    #[serde(default)]
    pub checks: Vec<Check>,
    /// Modelos que no pasaron su control (se informan, no se usan).
    #[serde(skip)]
    pub disabled: Vec<String>,
}

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= 1e-9 + 1e-9 * expected.abs()
}

impl Bundle {
    pub fn parse(bytes: &[u8]) -> Result<Self, ModelError> {
        let mut bundle: Bundle = serde_json::from_slice(bytes)
            .map_err(|e| ModelError(format!("modelos aprendidos ilegibles: {e}")))?;
        if bundle.schema != SCHEMA {
            return Err(ModelError(format!(
                "formato de modelos desconocido: {}",
                bundle.schema
            )));
        }
        if crate::domain::profile(&bundle.crop_type).is_none() {
            return Err(ModelError(format!(
                "especie desconocida: {}",
                bundle.crop_type
            )));
        }
        if bundle.features != FEATURES {
            return Err(ModelError(format!(
                "se esperaban {FEATURES} variables y llegaron {}",
                bundle.features
            )));
        }
        bundle.crop_type = bundle.crop_type.to_uppercase();
        bundle.validate_and_check();
        Ok(bundle)
    }

    /// Valida cada modelo y lo contrasta con los casos de control; lo que falle se deshabilita.
    fn validate_and_check(&mut self) {
        let checks = std::mem::take(&mut self.checks);
        let rows: Vec<&Check> = checks
            .iter()
            .filter(|c| c.x.len() == FEATURES && c.x.iter().all(|v| v.is_finite()))
            .collect();
        for card in &mut self.cards {
            let Some(estimator) = &card.estimator else {
                continue;
            };
            let problem = estimator.validate(FEATURES).err().map(|e| e.0).or_else(|| {
                rows.iter().find_map(|check| {
                    let expected = match card.task.as_str() {
                        "needs_water" => check.needs_water,
                        "overheat" => check.overheat,
                        "moisture_1h" => check.moisture_1h,
                        _ => None,
                    }?;
                    let actual = estimator.evaluate(&check.x);
                    (!close(actual, expected)).then(|| {
                        format!("no coincide con scikit-learn ({actual} frente a {expected})")
                    })
                })
            });
            if let Some(reason) = problem {
                self.disabled.push(format!("{}: {reason}", card.task));
                card.estimator = None;
            }
        }
        if let Some(states) = &self.states {
            let problem = states.validate().err().map(|e| e.0).or_else(|| {
                rows.iter().find_map(|check| {
                    let expected = check.state?;
                    let actual = states.assign(&check.x[..6]).map(|c| c.id);
                    (actual != Some(expected))
                        .then(|| "el estado asignado no coincide con scikit-learn".to_string())
                })
            });
            if let Some(reason) = problem {
                self.disabled.push(format!("states: {reason}"));
                self.states = None;
            }
        }
        if let Some(anomaly) = &self.anomaly {
            let problem = anomaly.forest.validate(6).err().map(|e| e.0).or_else(|| {
                rows.iter().find_map(|check| {
                    let expected = check.anomaly?;
                    let actual = anomaly.forest.decision(&check.x[..6]);
                    (!close(actual, expected))
                        .then(|| "el detector de atípicos no coincide con scikit-learn".to_string())
                })
            });
            if let Some(reason) = problem {
                self.disabled.push(format!("anomaly: {reason}"));
                self.anomaly = None;
            }
        }
        let mut broken = Vec::new();
        for (task, adapter) in &self.lora {
            if let Err(e) = adapter.validate(FEATURES) {
                broken.push(task.clone());
                self.disabled.push(format!("lora {task}: {}", e.0));
            }
        }
        for task in broken {
            self.lora.remove(&task);
        }
    }

    pub fn card(&self, task: &str) -> Option<&Card> {
        self.cards.iter().find(|c| c.task == task)
    }

    /// Hay algo que aportar a la evaluación (como `TypeState.usable` en Python).
    pub fn usable(&self) -> bool {
        self.cards.iter().any(Card::usable) || self.states.is_some()
    }

    pub fn trained_cards(&self) -> usize {
        self.cards.iter().filter(|c| c.usable()).count()
    }
}

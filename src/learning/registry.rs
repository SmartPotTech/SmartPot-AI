//! Registro de modelos aprendidos con recarga en caliente.
//!
//! Las evaluaciones leen una instantánea inmutable (`ArcSwap`): recargar no bloquea ni pausa ninguna evaluación en
//! curso. La recarga lee `{ESPECIE}_inference.json` de la carpeta compartida con SmartPot-Trainer, verifica el
//! manifiesto (`{ESPECIE}_manifest.json`) y reemplaza la especie de una sola vez.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arc_swap::ArcSwap;
use ed25519_dalek::VerifyingKey;
use serde::Serialize;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::domain::{CropProfile, Parameter, Values, PROFILES};
use crate::format::{py_round, thousands};
use crate::learning::bundle::Bundle;
use crate::learning::features::live_features;
use crate::learning::verifier::{self, Verdict};
use crate::schema::response::{
    iso_utc, LearnedAnomaly, LearnedMoisture, LearnedPrediction, Learning, OperatingState,
};

/// Tope de tamaño de un archivo de modelos.
const MAX_BUNDLE_BYTES: u64 = 64 * 1024 * 1024;
/// Tope del manifiesto firmado: un JSON pequeño no puede agotar la memoria.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

pub fn bundle_name(crop_type: &str) -> String {
    format!("{crop_type}_inference.json")
}

pub fn manifest_name(crop_type: &str) -> String {
    format!("{crop_type}_manifest.json")
}

/// Hechos que lo aprendido aporta al sistema experto y al agente.
#[derive(Debug, Clone, Copy, Default)]
pub struct LearnedFacts {
    pub needs_water: Option<f64>,
    pub overheat: Option<f64>,
    pub unusual: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rejection {
    pub crop_type: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReloadReport {
    pub reloaded: Vec<String>,
    pub rejected: Vec<Rejection>,
    /// Modelos deshabilitados porque no pasaron sus casos de control.
    pub disabled: Vec<String>,
}

pub struct Registry {
    dir: Option<PathBuf>,
    trusted: Option<VerifyingKey>,
    models: ArcSwap<HashMap<String, Arc<Bundle>>>,
    writer: Mutex<()>,
}

/// Lectura actual con su historial, para las variables de los modelos aprendidos.
pub struct LiveReading<'a> {
    pub values: &'a Values,
    pub hour: Option<u8>,
    /// Segundos epoch de la lectura.
    pub moment: f64,
    /// Lecturas anteriores con hora (segundos epoch).
    pub history: Vec<(f64, Values)>,
}

/// Conteos que informa el Trainer para el mensaje de lo aprendido.
#[derive(Debug, Clone, Copy, Default)]
pub struct Counts {
    pub readings: Option<i64>,
    pub min_samples: Option<i64>,
}

impl Registry {
    pub fn new(dir: Option<PathBuf>, trusted: Option<VerifyingKey>) -> Self {
        Self {
            dir,
            trusted,
            models: ArcSwap::from_pointee(HashMap::new()),
            writer: Mutex::new(()),
        }
    }

    pub fn directory(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn is_signed(&self) -> bool {
        self.trusted.is_some()
    }

    pub fn crop_types(&self) -> Vec<String> {
        let mut types: Vec<String> = self.models.load().keys().cloned().collect();
        types.sort();
        types
    }

    pub fn get(&self, crop_type: &str) -> Option<Arc<Bundle>> {
        self.models.load().get(crop_type).cloned()
    }

    /// Instala un paquete ya validado (pruebas y cargas en memoria).
    pub fn install(&self, bundle: Bundle) {
        let mut next = (**self.models.load()).clone();
        next.insert(bundle.crop_type.clone(), Arc::new(bundle));
        self.models.store(Arc::new(next));
    }

    /// Lee y verifica el paquete de una especie desde la carpeta compartida.
    fn read(&self, crop_type: &str) -> Result<Option<Bundle>, String> {
        let Some(dir) = &self.dir else {
            return Ok(None);
        };
        let path = dir.join(bundle_name(crop_type));
        let Ok(metadata) = std::fs::metadata(&path) else {
            return Ok(None);
        };
        if metadata.len() > MAX_BUNDLE_BYTES {
            return Err("model file exceeds maximum allowed size".into());
        }
        let bytes =
            std::fs::read(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let manifest_path = dir.join(manifest_name(crop_type));
        match std::fs::metadata(&manifest_path) {
            Ok(metadata) if metadata.len() > MAX_MANIFEST_BYTES => {
                return Err("manifest file exceeds maximum allowed size".into())
            }
            _ => {}
        }
        match std::fs::read(&manifest_path) {
            Ok(manifest) => {
                let verdict = verifier::verify(
                    &manifest,
                    &bundle_name(crop_type),
                    &bytes,
                    self.trusted.as_ref(),
                )?;
                if verdict == Verdict::HashOnly {
                    tracing::debug!("{crop_type}: hash verified; without a trusted key the signature is not checked");
                }
            }
            Err(_) if self.trusted.is_some() => return Err("model has no signed manifest".into()),
            Err(_) => {
                warn!(
                    "{crop_type}: models without manifest; loading without integrity verification"
                )
            }
        }
        let bundle = Bundle::parse(&bytes).map_err(|e| e.0)?;
        if bundle.crop_type != crop_type {
            return Err(format!(
                "file for {crop_type} contains models for {}",
                bundle.crop_type
            ));
        }
        Ok(Some(bundle))
    }

    /// Recarga una especie (o todas) sin detener las evaluaciones.
    pub async fn reload(self: &Arc<Self>, crop_type: Option<String>) -> ReloadReport {
        let _guard = self.writer.lock().await;
        let targets: Vec<String> = match crop_type {
            Some(kind) => vec![kind.to_uppercase()],
            None => PROFILES.iter().map(|p| p.kind.to_string()).collect(),
        };
        let this = Arc::clone(self);
        let loaded = tokio::task::spawn_blocking(move || {
            targets
                .into_iter()
                .map(|kind| {
                    let result = this.read(&kind);
                    (kind, result)
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();

        let mut report = ReloadReport::default();
        let mut next = (**self.models.load()).clone();
        for (kind, result) in loaded {
            match result {
                Ok(Some(bundle)) => {
                    report
                        .disabled
                        .extend(bundle.disabled.iter().map(|d| format!("{kind} {d}")));
                    for problem in &bundle.disabled {
                        warn!("{kind}: model disabled ({problem})");
                    }
                    next.insert(kind.clone(), Arc::new(bundle));
                    report.reloaded.push(kind);
                }
                Ok(None) => {}
                Err(reason) => {
                    warn!("{kind}: models rejected ({reason})");
                    report.rejected.push(Rejection {
                        crop_type: kind,
                        reason,
                    });
                }
            }
        }
        self.models.store(Arc::new(next));
        if !report.reloaded.is_empty() {
            info!("Learned models in memory: {}", report.reloaded.join(", "));
        }
        report
    }

    /// Lo aprendido de las lecturas reales de la especie; nunca impide la evaluación.
    pub fn infer(
        &self,
        profile: &CropProfile,
        reading: &LiveReading<'_>,
        counts: Counts,
        default_min_samples: i64,
    ) -> (Learning, LearnedFacts) {
        let name = profile.name.to_lowercase();
        let bundle = self.get(profile.kind);
        let readings = counts
            .readings
            .or(bundle.as_ref().map(|b| b.readings_at_training))
            .unwrap_or(0);
        let Some(bundle) = bundle.filter(|b| b.usable()) else {
            let min_samples = counts.min_samples.unwrap_or(default_min_samples);
            let learning = Learning {
                source: "BASE",
                readings,
                trained_at: None,
                message: format!(
                    "Still learning from {name} crops: {readings} real readings collected, and the first training starts at {min_samples}."
                ),
                predictions: Vec::new(),
                moisture: None,
                state: None,
                anomaly: None,
            };
            return (learning, LearnedFacts::default());
        };

        let mut facts = LearnedFacts::default();
        let mut predictions = Vec::new();
        let mut moisture = None;
        let mut state = None;
        let mut anomaly = None;
        let values = reading.values;
        if let Some(x) = live_features(
            profile,
            values,
            reading.hour,
            reading.moment,
            &reading.history,
        ) {
            let soil = x[Parameter::SoilMoisture.index()];
            let temperature = x[Parameter::Temperature.index()];
            for (task, applicable) in [
                ("needs_water", soil >= 0.0),
                ("overheat", temperature <= 1.0),
            ] {
                let Some(card) = bundle.card(task).filter(|c| c.usable() && applicable) else {
                    continue;
                };
                let estimator = card.estimator.as_ref().expect("usable");
                let champion = card.model.clone().unwrap_or_default();
                let (probability, model) = match bundle.lora.get(task).filter(|a| a.is_active()) {
                    Some(adapter) => (adapter.probability(&x), format!("{champion} + LoRA")),
                    None => (estimator.evaluate(&x), champion),
                };
                let probability = py_round(probability, 3);
                match task {
                    "needs_water" => facts.needs_water = Some(probability),
                    _ => facts.overheat = Some(probability),
                }
                predictions.push(LearnedPrediction {
                    name: if task == "needs_water" {
                        "needs_water"
                    } else {
                        "overheat"
                    },
                    label: card.label.clone(),
                    probability,
                    model,
                    metric: card.metric.clone(),
                    score: card.holdout,
                });
            }
            if let (Some(card), Some(current)) = (
                bundle.card("moisture_1h").filter(|c| c.usable()),
                values[Parameter::SoilMoisture.index()],
            ) {
                let delta = card.estimator.as_ref().expect("usable").evaluate(&x);
                moisture = Some(LearnedMoisture {
                    expected_in_1h: py_round((current + delta).clamp(0.0, 100.0), 1),
                    model: card.model.clone().unwrap_or_default(),
                    mae: card.holdout,
                });
            }
            let positions = &x[..6];
            if let Some(cluster) = bundle.states.as_ref().and_then(|s| s.assign(positions)) {
                state = Some(OperatingState {
                    label: cluster.label.clone(),
                    description: cluster.description.clone(),
                    share: cluster.share,
                });
            }
            if let Some(detector) = &bundle.anomaly {
                let (score, unusual) = detector.score(positions);
                if unusual {
                    facts.unusual = Some(score);
                }
                anomaly = Some(LearnedAnomaly { score, unusual });
            }
        }
        let learning = Learning {
            source: "LEARNED",
            readings,
            trained_at: bundle.trained_at.filter(|t| *t != 0.0).map(iso_utc),
            message: format!(
                "Learned from {} real {name} readings: {} of 3 supervised models trained.",
                thousands(readings),
                bundle.trained_cards()
            ),
            predictions,
            moisture,
            state,
            anomaly,
        };
        (learning, facts)
    }
}

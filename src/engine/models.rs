//! Modelos base pre-empacados (los mismos del servicio en Python, semilla 42), sin entrenamiento al arrancar.
//!
//! Las variables se normalizan respecto al perfil de cada especie (0 = mínimo ideal, 1 = máximo ideal), así un mismo
//! modelo sirve para todos los cultivos:
//!
//! - Regresión logística: necesidad de ventilación a partir de temperatura y humedad.
//! - Red neuronal (MLP): necesidad de corregir el pH a partir de pH y nutrientes.
//! - Isolation Forest: qué tan improbable es físicamente la lectura, combinado con saltos bruscos frente al historial.
//!
//! Los parámetros vienen de `models/base_models.json` (lo genera `tools/export_base_models.py`) y quedan dentro del
//! binario: cargarlos toma unos milisegundos y no depende de archivos externos.

use serde::Deserialize;

use crate::domain::{CropProfile, Parameter, Values};
use crate::format::py_round;
use crate::ml::{IsolationForest, Model, ModelError, Pipeline, Scaler};
use crate::numeric::{mean, std};

const EMBEDDED: &str = include_str!("../../models/base_models.json");
pub const SPIKE_Z: f64 = 4.0;

#[derive(Debug, Deserialize)]
struct LinearSpec {
    scaler: Scaler,
    #[serde(flatten)]
    model: Model,
}

#[derive(Debug, Deserialize)]
struct BaseFile {
    schema: String,
    metrics: serde_json::Map<String, serde_json::Value>,
    ventilation: LinearSpec,
    ph_correction: LinearSpec,
    anomaly: IsolationForest,
}

/// Posición de cada valor dentro de su rango ideal (0 a 1 dentro del rango).
pub fn normalize(profile: &CropProfile, values: &Values) -> Values {
    let mut positions = [None; 6];
    for parameter in Parameter::ALL {
        if let Some(value) = values[parameter.index()] {
            let range = profile.range(parameter);
            positions[parameter.index()] = Some((value - range.min) / (range.max - range.min));
        }
    }
    positions
}

#[derive(Debug)]
pub struct BaseModels {
    pub metrics: Vec<(String, f64)>,
    ventilation: Pipeline,
    ph_correction: Pipeline,
    anomaly: IsolationForest,
}

impl BaseModels {
    /// Modelos incluidos en el binario.
    pub fn embedded() -> Result<Self, ModelError> {
        Self::from_json(EMBEDDED)
    }

    pub fn from_json(text: &str) -> Result<Self, ModelError> {
        let file: BaseFile = serde_json::from_str(text)
            .map_err(|e| ModelError(format!("unreadable base models: {e}")))?;
        if file.schema != "smartpot.base-models/1" {
            return Err(ModelError(format!(
                "unknown base models schema: {}",
                file.schema
            )));
        }
        let ventilation = Pipeline {
            scaler: Some(file.ventilation.scaler),
            model: file.ventilation.model,
        };
        let ph_correction = Pipeline {
            scaler: Some(file.ph_correction.scaler),
            model: file.ph_correction.model,
        };
        ventilation.validate(4)?;
        ph_correction.validate(2)?;
        file.anomaly.validate(6)?;
        let metrics = file
            .metrics
            .into_iter()
            .filter_map(|(name, value)| value.as_f64().map(|v| (name, v)))
            .collect();
        Ok(Self {
            metrics,
            ventilation,
            ph_correction,
            anomaly: file.anomaly,
        })
    }

    /// Necesidad de ventilación; el exceso sobre el máximo ideal vuelve lineal la regla «calor o humedad altos».
    pub fn ventilation_probability(&self, positions: &Values) -> Option<f64> {
        let t = positions[Parameter::Temperature.index()]?;
        let h = positions[Parameter::Humidity.index()]?;
        Some(
            self.ventilation
                .evaluate(&[t, h, (t - 1.0).max(0.0), (h - 1.0).max(0.0)]),
        )
    }

    pub fn ph_correction_probability(&self, positions: &Values) -> Option<f64> {
        let ph = positions[Parameter::Ph.index()]?;
        let tds = positions[Parameter::Tds.index()].unwrap_or(0.5);
        Some(self.ph_correction.evaluate(&[ph, tds]))
    }

    /// `decision_function` del Isolation Forest para un vector de seis posiciones.
    pub fn anomaly_decision(&self, vector: &[f64]) -> f64 {
        self.anomaly.decision(vector)
    }

    /// Probabilidad de lectura atípica: el bosque o un salto brusco frente al historial, lo que sea mayor.
    pub fn anomaly_probability(&self, positions: &Values, history: &[Values]) -> f64 {
        let vector: Vec<f64> = positions.iter().map(|p| p.unwrap_or(0.5)).collect();
        let score = self.anomaly_decision(&vector);
        let forest = 1.0 / (1.0 + (25.0 * score).exp());
        let spike = spike_probability(positions, history);
        py_round(if spike > forest { spike } else { forest }, 3)
    }
}

fn spike_probability(positions: &Values, history: &[Values]) -> f64 {
    if history.len() < 10 {
        return 0.0;
    }
    let mut worst: f64 = 0.0;
    for parameter in Parameter::ALL {
        let Some(value) = positions[parameter.index()] else {
            continue;
        };
        let series: Vec<f64> = history
            .iter()
            .filter_map(|h| h[parameter.index()])
            .collect();
        if series.len() < 10 {
            continue;
        }
        let spread = std(&series).max(0.05);
        let distance = (value - mean(&series)).abs() / spread;
        if distance > worst {
            worst = distance;
        }
    }
    if worst > SPIKE_Z {
        ((worst - SPIKE_Z) / SPIKE_Z + 0.5).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_models_load_and_report_their_metrics() {
        let models = BaseModels::embedded().unwrap();
        let names: Vec<&str> = models.metrics.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "ventilation_accuracy",
                "ph_correction_accuracy",
                "anomaly_detection_rate"
            ]
        );
        assert!(models.metrics.iter().all(|(_, v)| *v > 0.8));
    }

    #[test]
    fn a_sudden_jump_raises_the_anomaly() {
        let models = BaseModels::embedded().unwrap();
        let calm: Vec<Values> = (0..12)
            .map(|i| {
                [
                    Some(0.5 + f64::from(i % 3) * 0.01),
                    None,
                    None,
                    None,
                    None,
                    None,
                ]
            })
            .collect();
        let jump = [Some(3.0), None, None, None, None, None];
        assert_eq!(models.anomaly_probability(&jump, &calm), 1.0);
    }
}

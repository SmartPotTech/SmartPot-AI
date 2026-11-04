//! Evaluación nativa de los modelos entrenados con scikit-learn, sin pickle ni joblib.
//!
//! Los modelos llegan como parámetros en JSON (escalado, coeficientes, capas, árboles, vecinos) y se evalúan con las
//! mismas operaciones y en el mismo orden que scikit-learn: los árboles de decisión, los bosques y el Isolation
//! Forest comparan la entrada convertida a `f32` (scikit-learn la convierte así antes de recorrerlos); el gradient
//! boosting por histogramas compara en `f64` y envía los valores ausentes por el lado aprendido.

pub mod kmeans;
pub mod lora;

use serde::Deserialize;

use crate::numeric::{expit, mean};

/// Error de un modelo mal formado.
#[derive(Debug, thiserror::Error, PartialEq)]
#[error("{0}")]
pub struct ModelError(pub String);

fn invalid(message: impl Into<String>) -> ModelError {
    ModelError(message.into())
}

/// `StandardScaler`: `(x - media) / escala`.
#[derive(Debug, Clone, Deserialize)]
pub struct Scaler {
    pub mean: Vec<f64>,
    pub scale: Vec<f64>,
}

impl Scaler {
    pub fn transform(&self, x: &[f64]) -> Vec<f64> {
        x.iter()
            .zip(&self.mean)
            .zip(&self.scale)
            .map(|((v, m), s)| (v - m) / s)
            .collect()
    }

    fn validate(&self, features: usize) -> Result<(), ModelError> {
        if self.mean.len() != features || self.scale.len() != features {
            return Err(invalid(format!("scaler expects {features} features")));
        }
        if self.scale.iter().chain(&self.mean).any(|v| !v.is_finite()) || self.scale.contains(&0.0)
        {
            return Err(invalid("scaler contains invalid values"));
        }
        Ok(())
    }
}

/// Árbol binario con nodos en arreglos paralelos; una hoja tiene `left == -1`.
#[derive(Debug, Clone, Deserialize)]
pub struct Tree {
    pub left: Vec<i64>,
    pub right: Vec<i64>,
    pub feature: Vec<i64>,
    pub threshold: Vec<f64>,
    pub value: Vec<f64>,
    /// Solo en el gradient boosting: hacia dónde va un valor ausente.
    #[serde(default)]
    pub missing_left: Vec<bool>,
}

impl Tree {
    /// Valida índices y que cada hijo esté después de su padre (así recorrerlo siempre termina).
    pub fn validate(&self, features: usize) -> Result<(), ModelError> {
        let n = self.left.len();
        if n == 0
            || [
                self.right.len(),
                self.feature.len(),
                self.threshold.len(),
                self.value.len(),
            ]
            .iter()
            .any(|l| *l != n)
        {
            return Err(invalid("tree arrays have different lengths"));
        }
        if !self.missing_left.is_empty() && self.missing_left.len() != n {
            return Err(invalid("tree missing-value array has a different length"));
        }
        for i in 0..n {
            if self.left[i] < 0 {
                if !self.value[i].is_finite() {
                    return Err(invalid("leaf has an invalid value"));
                }
                continue;
            }
            let (l, r) = (self.left[i], self.right[i]);
            let inside = |child: i64| child > i as i64 && (child as usize) < n;
            if !inside(l) || !inside(r) {
                return Err(invalid("tree has out-of-order children"));
            }
            if self.feature[i] < 0
                || self.feature[i] as usize >= features
                || self.threshold[i].is_nan()
            {
                return Err(invalid("tree references a feature out of range"));
            }
        }
        Ok(())
    }

    /// Hoja alcanzada por `x` con la regla de scikit-learn `x[f] <= umbral` (entrada ya convertida a `f32`).
    pub fn leaf(&self, x: &[f64]) -> f64 {
        let mut node = 0usize;
        while self.left[node] >= 0 {
            let value = x[self.feature[node] as usize];
            node = if value <= self.threshold[node] {
                self.left[node]
            } else {
                self.right[node]
            } as usize;
        }
        self.value[node]
    }

    /// Recorrido del gradient boosting: en `f64` y con ausentes por el lado aprendido.
    pub fn leaf_with_missing(&self, x: &[f64]) -> f64 {
        let mut node = 0usize;
        while self.left[node] >= 0 {
            let value = x[self.feature[node] as usize];
            let left = if value.is_nan() {
                self.missing_left.get(node).copied().unwrap_or(false)
            } else {
                value <= self.threshold[node]
            };
            node = if left {
                self.left[node]
            } else {
                self.right[node]
            } as usize;
        }
        self.value[node]
    }
}

fn as_f32(x: &[f64]) -> Vec<f64> {
    x.iter().map(|v| f64::from(*v as f32)).collect()
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    Identity,
    Logistic,
    Tanh,
    Relu,
}

impl Activation {
    fn apply(self, values: &mut [f64]) {
        for v in values {
            *v = match self {
                Activation::Identity => *v,
                Activation::Logistic => expit(*v),
                Activation::Tanh => v.tanh(),
                Activation::Relu => v.max(0.0),
            };
        }
    }
}

/// Capa densa: `pesos` con forma (entradas × salidas).
#[derive(Debug, Clone, Deserialize)]
pub struct Layer {
    pub weights: Vec<Vec<f64>>,
    pub bias: Vec<f64>,
}

/// Qué devuelve el modelo: probabilidad de la clase positiva o un valor (regresión).
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    Probability,
    Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Model {
    /// Regresión logística binaria.
    Logistic { coef: Vec<f64>, intercept: f64 },
    /// Regresión lineal (ridge).
    Linear { coef: Vec<f64>, intercept: f64 },
    /// Perceptrón multicapa.
    Mlp {
        hidden_activation: Activation,
        output_activation: Activation,
        layers: Vec<Layer>,
    },
    /// Árbol de decisión o bosque aleatorio: promedio de las hojas (probabilidad o valor).
    Forest { trees: Vec<Tree> },
    /// Gradient boosting por histogramas: base + suma de hojas.
    HistGradientBoosting {
        baseline: f64,
        trees: Vec<Tree>,
        output: Output,
    },
    /// K vecinos más cercanos con pesos uniformes.
    Knn {
        k: usize,
        fit_x: Vec<Vec<f64>>,
        targets: Vec<f64>,
        output: Output,
    },
    /// Línea base constante.
    Constant { value: f64 },
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut total = 0.0;
    for (x, y) in a.iter().zip(b) {
        total += x * y;
    }
    total
}

impl Model {
    pub fn validate(&self, features: usize) -> Result<(), ModelError> {
        match self {
            Model::Logistic { coef, intercept } | Model::Linear { coef, intercept } => {
                if coef.len() != features
                    || !intercept.is_finite()
                    || coef.iter().any(|c| !c.is_finite())
                {
                    return Err(invalid(format!(
                        "linear model: expected {features} finite coefficients"
                    )));
                }
            }
            Model::Mlp { layers, .. } => {
                let mut width = features;
                for layer in layers {
                    let outputs = layer.bias.len();
                    if layer.weights.len() != width
                        || layer.weights.iter().any(|row| row.len() != outputs)
                    {
                        return Err(invalid("neural network has incompatible layers"));
                    }
                    if layer
                        .weights
                        .iter()
                        .flatten()
                        .chain(&layer.bias)
                        .any(|v| !v.is_finite())
                    {
                        return Err(invalid("neural network has invalid weights"));
                    }
                    width = outputs;
                }
                if layers.is_empty() || width != 1 {
                    return Err(invalid("neural network must end in a single output"));
                }
            }
            Model::Forest { trees } | Model::HistGradientBoosting { trees, .. } => {
                if trees.is_empty() {
                    return Err(invalid("empty tree ensemble"));
                }
                for tree in trees {
                    tree.validate(features)?;
                }
            }
            Model::Knn {
                k, fit_x, targets, ..
            } => {
                if *k == 0 || *k > fit_x.len() || fit_x.len() != targets.len() {
                    return Err(invalid("nearest neighbors model has incomplete data"));
                }
                if fit_x
                    .iter()
                    .any(|row| row.len() != features || row.iter().any(|v| !v.is_finite()))
                {
                    return Err(invalid("nearest neighbors model has invalid rows"));
                }
            }
            Model::Constant { value } => {
                if !value.is_finite() {
                    return Err(invalid("invalid constant"));
                }
            }
        }
        Ok(())
    }

    /// Probabilidad de la clase positiva (clasificadores) o valor estimado (regresores).
    pub fn evaluate(&self, x: &[f64]) -> f64 {
        match self {
            Model::Logistic { coef, intercept } => expit(dot(x, coef) + intercept),
            Model::Linear { coef, intercept } => dot(x, coef) + intercept,
            Model::Mlp {
                hidden_activation,
                output_activation,
                layers,
            } => {
                let mut activation = x.to_vec();
                for (i, layer) in layers.iter().enumerate() {
                    let mut next = vec![0.0; layer.bias.len()];
                    for (j, slot) in next.iter_mut().enumerate() {
                        let mut total = 0.0;
                        for (a, row) in activation.iter().zip(&layer.weights) {
                            total += a * row[j];
                        }
                        *slot = total + layer.bias[j];
                    }
                    if i + 1 < layers.len() {
                        hidden_activation.apply(&mut next);
                    }
                    activation = next;
                }
                output_activation.apply(&mut activation);
                activation[0]
            }
            Model::Forest { trees } => {
                let x32 = as_f32(x);
                let mut total = 0.0;
                for tree in trees {
                    total += tree.leaf(&x32);
                }
                total / trees.len() as f64
            }
            Model::HistGradientBoosting {
                baseline,
                trees,
                output,
            } => {
                let mut raw = 0.0 + baseline;
                for tree in trees {
                    raw += tree.leaf_with_missing(x);
                }
                match output {
                    Output::Probability => expit(raw),
                    Output::Value => raw,
                }
            }
            Model::Knn {
                k,
                fit_x,
                targets,
                output,
            } => {
                let mut distances: Vec<(f64, usize)> = fit_x
                    .iter()
                    .enumerate()
                    .map(|(i, row)| {
                        let mut total = 0.0;
                        for (a, b) in x.iter().zip(row) {
                            total += (a - b) * (a - b);
                        }
                        (total, i)
                    })
                    .collect();
                let k = (*k).min(distances.len());
                distances
                    .select_nth_unstable_by(k - 1, |a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let nearest = &mut distances[..k];
                nearest.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                match output {
                    Output::Probability => {
                        nearest.iter().filter(|(_, i)| targets[*i] == 1.0).count() as f64 / k as f64
                    }
                    Output::Value => {
                        let values: Vec<f64> = nearest.iter().map(|(_, i)| targets[*i]).collect();
                        mean(&values)
                    }
                }
            }
            Model::Constant { value } => *value,
        }
    }
}

/// Escalado opcional seguido del modelo, como un `Pipeline` de scikit-learn.
#[derive(Debug, Clone, Deserialize)]
pub struct Pipeline {
    #[serde(default)]
    pub scaler: Option<Scaler>,
    pub model: Model,
}

impl Pipeline {
    pub fn validate(&self, features: usize) -> Result<(), ModelError> {
        if let Some(scaler) = &self.scaler {
            scaler.validate(features)?;
        }
        self.model.validate(features)
    }

    pub fn evaluate(&self, x: &[f64]) -> f64 {
        match &self.scaler {
            Some(scaler) => self.model.evaluate(&scaler.transform(x)),
            None => self.model.evaluate(x),
        }
    }
}

/// Isolation Forest: cada hoja guarda `profundidad + largo promedio - 1`, como lo suma scikit-learn.
#[derive(Debug, Clone, Deserialize)]
pub struct IsolationForest {
    pub trees: Vec<Tree>,
    pub denominator: f64,
    pub offset: f64,
}

impl IsolationForest {
    pub fn validate(&self, features: usize) -> Result<(), ModelError> {
        if self.trees.is_empty()
            || !self.denominator.is_finite()
            || self.denominator <= 0.0
            || !self.offset.is_finite()
        {
            return Err(invalid("incomplete Isolation Forest"));
        }
        for tree in &self.trees {
            tree.validate(features)?;
        }
        Ok(())
    }

    /// `decision_function`: negativo es atípico.
    pub fn decision(&self, x: &[f64]) -> f64 {
        let x32 = as_f32(x);
        let mut depth = 0.0;
        for tree in &self.trees {
            depth += tree.leaf(&x32);
        }
        -(2f64.powf(-(depth / self.denominator))) - self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stump(threshold: f64, left: f64, right: f64) -> Tree {
        Tree {
            left: vec![1, -1, -1],
            right: vec![2, -1, -1],
            feature: vec![0, -2, -2],
            threshold: vec![threshold, -2.0, -2.0],
            value: vec![0.0, left, right],
            missing_left: vec![true, false, false],
        }
    }

    #[test]
    fn trees_compare_in_f32() {
        // 0.1 en f32 es un poco mayor que 0.1 en f64: con umbral 0.1 (f64) la entrada va a la derecha.
        let forest = Model::Forest {
            trees: vec![stump(0.1, 0.0, 1.0)],
        };
        assert_eq!(forest.evaluate(&[0.1]), 1.0);
        let boosting = Model::HistGradientBoosting {
            baseline: 0.0,
            trees: vec![stump(0.1, -1.0, 1.0)],
            output: Output::Value,
        };
        assert_eq!(boosting.evaluate(&[0.1]), -1.0);
        assert_eq!(boosting.evaluate(&[f64::NAN]), -1.0);
    }

    #[test]
    fn rejects_trees_that_could_loop() {
        let mut tree = stump(0.5, 0.0, 1.0);
        tree.left[0] = 0;
        assert!(tree.validate(1).is_err());
        assert!(stump(0.5, 0.0, 1.0).validate(1).is_ok());
        assert!(stump(0.5, 0.0, 1.0).validate(0).is_err());
    }

    #[test]
    fn knn_votes_and_averages() {
        let fit_x = vec![vec![0.0], vec![1.0], vec![2.0], vec![10.0]];
        let classifier = Model::Knn {
            k: 3,
            fit_x: fit_x.clone(),
            targets: vec![1.0, 1.0, 0.0, 0.0],
            output: Output::Probability,
        };
        assert!((classifier.evaluate(&[0.5]) - 2.0 / 3.0).abs() < 1e-12);
        let regressor = Model::Knn {
            k: 2,
            fit_x,
            targets: vec![4.0, 6.0, 8.0, 100.0],
            output: Output::Value,
        };
        assert_eq!(regressor.evaluate(&[0.4]), 5.0);
    }

    #[test]
    fn mlp_forward_pass() {
        let model = Model::Mlp {
            hidden_activation: Activation::Relu,
            output_activation: Activation::Identity,
            layers: vec![
                Layer {
                    weights: vec![vec![1.0, -1.0]],
                    bias: vec![0.0, 0.0],
                },
                Layer {
                    weights: vec![vec![2.0], vec![3.0]],
                    bias: vec![1.0],
                },
            ],
        };
        assert!(model.validate(1).is_ok());
        assert_eq!(model.evaluate(&[2.0]), 5.0);
        assert_eq!(model.evaluate(&[-2.0]), 7.0);
    }
}

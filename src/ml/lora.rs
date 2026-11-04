//! Adaptadores de bajo rango (LoRA) por especie, entrenados en línea por SmartPot-Trainer.
//!
//! Peso efectivo `W = W0 + (α / r) · B · A` y sesgo `b0 + sesgo`; la probabilidad es `σ(x · W + b)` con `z` acotado a
//! ±30, igual que el adaptador en Python. Solo se usa si el adaptador recibió actualizaciones.

use serde::Deserialize;

use crate::ml::ModelError;
use crate::numeric::expit;

#[derive(Debug, Clone, Deserialize)]
pub struct Adapter {
    pub d_in: usize,
    pub k_out: usize,
    pub rank: usize,
    pub alpha: f64,
    /// d_in × k_out
    pub w0: Vec<Vec<f64>>,
    /// 1 × k_out
    pub b0: Vec<Vec<f64>>,
    /// rank × k_out
    pub a: Vec<Vec<f64>>,
    /// d_in × rank
    pub b: Vec<Vec<f64>>,
    /// 1 × k_out
    pub bias: Vec<Vec<f64>>,
    #[serde(default)]
    pub updates_count: u64,
}

fn shaped(matrix: &[Vec<f64>], rows: usize, cols: usize) -> bool {
    matrix.len() == rows
        && matrix
            .iter()
            .all(|row| row.len() == cols && row.iter().all(|v| v.is_finite()))
}

impl Adapter {
    pub fn validate(&self, features: usize) -> Result<(), ModelError> {
        let ok = self.d_in == features
            && self.k_out == 1
            && self.rank > 0
            && self.alpha.is_finite()
            && shaped(&self.w0, self.d_in, self.k_out)
            && shaped(&self.b0, 1, self.k_out)
            && shaped(&self.a, self.rank, self.k_out)
            && shaped(&self.b, self.d_in, self.rank)
            && shaped(&self.bias, 1, self.k_out);
        if ok {
            Ok(())
        } else {
            Err(ModelError("LoRA adapter has invalid dimensions".into()))
        }
    }

    pub fn is_active(&self) -> bool {
        self.updates_count > 0
    }

    /// `W0 + (α / r) · (B · A)`, columna de la única salida.
    pub fn effective_weight(&self) -> Vec<f64> {
        let scaling = self.alpha / self.rank as f64;
        (0..self.d_in)
            .map(|i| {
                let mut delta = 0.0;
                for r in 0..self.rank {
                    delta += self.b[i][r] * self.a[r][0];
                }
                self.w0[i][0] + scaling * delta
            })
            .collect()
    }

    pub fn probability(&self, x: &[f64]) -> f64 {
        let weight = self.effective_weight();
        let mut z = 0.0;
        for (value, w) in x.iter().zip(&weight) {
            z += value * w;
        }
        z += self.b0[0][0] + self.bias[0][0];
        expit(z.clamp(-30.0, 30.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_rank_update_changes_the_weight() {
        let adapter = Adapter {
            d_in: 2,
            k_out: 1,
            rank: 2,
            alpha: 1.0,
            w0: vec![vec![1.0], vec![-1.0]],
            b0: vec![vec![0.0]],
            a: vec![vec![1.0], vec![2.0]],
            b: vec![vec![0.5, 0.0], vec![0.0, 0.5]],
            bias: vec![vec![0.0]],
            updates_count: 3,
        };
        assert!(adapter.validate(2).is_ok());
        // ΔW = (1/2) · B · A = [0.25, 0.5]
        assert_eq!(adapter.effective_weight(), vec![1.25, -0.5]);
        assert!((adapter.probability(&[0.0, 0.0]) - 0.5).abs() < 1e-12);
        assert!(adapter.probability(&[1e6, 0.0]) < 1.0);
        assert!(adapter.validate(3).is_err());
    }
}

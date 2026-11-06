//! Filtro de Kalman 1D con compuerta de innovación para señales ruidosas (pH, TDS, temperatura, humedad).
//!
//! Predicción `x = x`, `P = P + Q·dt`; innovación `y = z - x` con varianza `S = P + R`. Si `|y| > k·√S` (por defecto
//! `k = 3`, tres desviaciones) tras el calentamiento, la lectura se descarta como falla de cableado y solo crece la
//! incertidumbre. Es una utilidad del motor: no cambia la respuesta de la evaluación.

#[derive(Debug, Clone)]
pub struct SensorFilter {
    pub estimate: f64,
    pub variance: f64,
    pub updates: u32,
    pub rejected: u32,
    pub last_innovation: f64,
    process_variance: f64,
    measurement_variance: f64,
    gate_sigma: f64,
    warmup: u32,
}

impl SensorFilter {
    pub fn new(initial: f64, process_variance: f64, measurement_variance: f64) -> Self {
        Self {
            estimate: initial,
            variance: 1.0,
            updates: 0,
            rejected: 0,
            last_innovation: 0.0,
            process_variance,
            measurement_variance,
            gate_sigma: 3.0,
            warmup: 5,
        }
    }

    pub fn with_gate(mut self, sigma: f64) -> Self {
        if sigma.is_finite() && sigma > 0.0 {
            self.gate_sigma = sigma;
        }
        self
    }

    /// Ciclo de predicción y corrección. Devuelve la estimación y si la lectura se aceptó.
    pub fn update(&mut self, measurement: f64, dt_seconds: f64) -> (f64, bool) {
        // Un NaN/Inf nunca debe envenenar ni la estimación ni la varianza.
        if !measurement.is_finite() || !dt_seconds.is_finite() {
            self.rejected += 1;
            return (self.estimate, false);
        }
        let dt = dt_seconds.clamp(0.1, 60.0);
        let predicted = self.variance + self.process_variance * dt;
        let innovation = measurement - self.estimate;
        let spread = predicted + self.measurement_variance;
        self.last_innovation = innovation;
        if self.updates > self.warmup && innovation.abs() > self.gate_sigma * spread.sqrt() {
            self.variance = predicted;
            self.rejected += 1;
            return (self.estimate, false);
        }
        let gain = predicted / spread;
        self.estimate += gain * innovation;
        self.variance = (1.0 - gain) * predicted;
        self.updates += 1;
        (self.estimate, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooths_noise_and_rejects_glitches() {
        let mut filter = SensorFilter::new(6.0, 1e-3, 0.05);
        let noisy = [6.1, 5.9, 6.05, 5.95, 6.02, 5.98, 6.01, 5.99];
        for value in noisy {
            assert!(filter.update(value, 1.0).1);
        }
        assert!((filter.estimate - 6.0).abs() < 0.05);
        let (estimate, accepted) = filter.update(14.0, 1.0);
        assert!(!accepted);
        assert!((estimate - 6.0).abs() < 0.05);
        assert_eq!(filter.rejected, 1);
    }

    #[test]
    fn non_finite_inputs_never_poison_the_filter() {
        let mut filter = SensorFilter::new(6.0, 1e-3, 0.05);
        for value in [6.1, 5.9, 6.05, 5.95, 6.02, 5.98] {
            assert!(filter.update(value, 1.0).1);
        }
        let estimate = filter.estimate;
        let variance = filter.variance;
        assert!(!filter.update(f64::NAN, 1.0).1);
        assert!(!filter.update(6.1, f64::NAN).1);
        assert!(!filter.update(6.1, f64::INFINITY).1);
        assert_eq!(filter.estimate, estimate);
        assert_eq!(filter.variance, variance);
        assert_eq!(filter.rejected, 3);
        assert!(filter.update(6.0, 1.0).0.is_finite());
    }
}

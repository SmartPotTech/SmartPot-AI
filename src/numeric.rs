//! Operaciones numéricas con el mismo orden de cálculo que NumPy.
//!
//! NumPy suma por pares (bloques de 8 acumuladores hasta 128 elementos y división recursiva por encima). Repetir ese
//! orden da exactamente los mismos bits en medias y desviaciones, así los redondeos posteriores coinciden con Python.

const BLOCK: usize = 128;

/// Suma por pares de NumPy (`np.sum` sobre un arreglo de `f64`).
pub fn pairwise_sum(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 8 {
        let mut total = 0.0;
        for value in values {
            total += value;
        }
        return total;
    }
    if n <= BLOCK {
        let mut acc = [0.0f64; 8];
        acc.copy_from_slice(&values[..8]);
        let limit = n - (n % 8);
        let mut i = 8;
        while i < limit {
            for (j, slot) in acc.iter_mut().enumerate() {
                *slot += values[i + j];
            }
            i += 8;
        }
        let mut total =
            ((acc[0] + acc[1]) + (acc[2] + acc[3])) + ((acc[4] + acc[5]) + (acc[6] + acc[7]));
        for value in &values[i..] {
            total += value;
        }
        return total;
    }
    let mut half = n / 2;
    half -= half % 8;
    pairwise_sum(&values[..half]) + pairwise_sum(&values[half..])
}

/// `sum()` de Python 3.12+ sobre `float`: suma compensada de Neumaier (no es la suma ingenua de izquierda a derecha).
pub fn py_sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut iter = values.into_iter();
    let Some(first) = iter.next() else {
        return 0.0;
    };
    let mut total = 0.0 + first;
    let mut compensation = 0.0;
    for x in iter {
        let t = total + x;
        if total.abs() >= x.abs() {
            compensation += (total - t) + x;
        } else {
            compensation += (x - t) + total;
        }
        total = t;
    }
    if compensation != 0.0 && compensation.is_finite() {
        total += compensation;
    }
    total
}

/// `np.mean`.
pub fn mean(values: &[f64]) -> f64 {
    pairwise_sum(values) / values.len() as f64
}

/// `np.std` con `ddof=0`.
pub fn std(values: &[f64]) -> f64 {
    let center = mean(values);
    let squares: Vec<f64> = values.iter().map(|v| (v - center) * (v - center)).collect();
    (pairwise_sum(&squares) / values.len() as f64).sqrt()
}

/// `np.median` por selección parcial en O(n), sin ordenar todo el arreglo.
pub fn median(values: &mut [f64]) -> f64 {
    let n = values.len();
    let mid = n / 2;
    let (left, upper, _) = values.select_nth_unstable_by(mid, f64::total_cmp);
    let upper = *upper;
    if n % 2 == 1 {
        return upper;
    }
    let lower = left.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (lower + upper) / 2.0
}

/// `np.sign`.
pub fn sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// `scipy.special.expit`.
pub fn expit(value: f64) -> f64 {
    1.0 / (1.0 + (-value).exp())
}

/// `np.round(value, 2)`: NumPy multiplica, redondea al par y divide (no es el redondeo decimal exacto de Python).
pub fn np_round(value: f64, digits: i32) -> f64 {
    let factor = 10f64.powi(digits);
    (value * factor).round_ties_even() / factor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_matches_numpy() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0, 4.0]), 2.5);
        assert_eq!(median(&mut [5.0, 1.0, 3.0]), 3.0);
        assert_eq!(median(&mut [7.0]), 7.0);
    }

    #[test]
    fn pairwise_sum_is_exact_for_small_and_large_arrays() {
        let small: Vec<f64> = (1..=7).map(f64::from).collect();
        assert_eq!(pairwise_sum(&small), 28.0);
        let large: Vec<f64> = (0..1000).map(|i| f64::from(i) * 0.1).collect();
        let expected: f64 = 49_950.0;
        assert!((pairwise_sum(&large) - expected).abs() < 1e-9);
    }

    #[test]
    fn python_sum_compensates_rounding() {
        // En Python 3.13, sum([0.1] * 10) == 1.0, mientras que la suma ingenua da 0.9999999999999999.
        assert_eq!(py_sum([0.1; 10]), 1.0);
        assert_eq!(py_sum(std::iter::empty()), 0.0);
    }

    #[test]
    fn std_is_population_deviation() {
        assert!((std(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]) - 2.0).abs() < 1e-12);
    }
}

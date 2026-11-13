//! K-Means determinista con el mismo recorrido que scikit-learn (`KMeans(n_init=10, random_state=42)`).
//!
//! Para que una flota dé los mismos grupos que el servicio en Python no basta con «un» K-Means: el resultado depende
//! de la inicialización. Aquí se repite el algoritmo de scikit-learn: generador MT19937 de `numpy.random.RandomState`
//! con la misma semilla, k-means++ «codicioso» (2 + ln k candidatos por paso, `choice` y `uniform` del generador
//! heredado), datos centrados, distancias como `‖x‖² - 2·x·c + ‖c‖²`, iteraciones de Lloyd con promedio por recíproco,
//! reubicación de grupos vacíos, tolerancia relativa a la varianza y la corrida de menor inercia. La silueta usa la
//! misma fórmula de distancias que `sklearn.metrics.silhouette_score`.
//!
//! Las funciones públicas devuelven `Result`: datos vacíos, filas de distinta longitud, valores no
//! finitos o un `k` fuera de rango se rechazan sin entrar en pánico.

use crate::ml::ModelError;

/// MT19937 con la semilla y la conversión a `f64` de `numpy.random.RandomState`.
#[derive(Debug, Clone)]
pub struct Mt19937 {
    state: [u32; 624],
    index: usize,
}

impl Mt19937 {
    pub fn new(seed: u32) -> Self {
        let mut state = [0u32; 624];
        state[0] = seed;
        for i in 1..624 {
            let previous = state[i - 1];
            state[i] = 1_812_433_253u32
                .wrapping_mul(previous ^ (previous >> 30))
                .wrapping_add(i as u32);
        }
        Self { state, index: 624 }
    }

    fn twist(&mut self) {
        for i in 0..624 {
            let y = (self.state[i] & 0x8000_0000) | (self.state[(i + 1) % 624] & 0x7fff_ffff);
            let mut next = self.state[(i + 397) % 624] ^ (y >> 1);
            if y & 1 != 0 {
                next ^= 0x9908_b0df;
            }
            self.state[i] = next;
        }
        self.index = 0;
    }

    pub fn next_u32(&mut self) -> u32 {
        if self.index >= 624 {
            self.twist();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// `random_sample()`: 53 bits a partir de dos números de 32 bits.
    pub fn next_f64(&mut self) -> f64 {
        let a = f64::from(self.next_u32() >> 5);
        let b = f64::from(self.next_u32() >> 6);
        (a * 67_108_864.0 + b) / 9_007_199_254_740_992.0
    }
}

#[derive(Debug, Clone)]
pub struct Clustering {
    pub labels: Vec<usize>,
    pub centers: Vec<Vec<f64>>,
    pub inertia: f64,
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut total = 0.0;
    for (x, y) in a.iter().zip(b) {
        total += x * y;
    }
    total
}

/// `_euclidean_dense_dense` de scikit-learn: bloques de cuatro y el resto.
fn squared_unrolled(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len();
    let mut total = 0.0;
    let blocks = n / 4;
    for i in 0..blocks {
        let j = i * 4;
        total += (a[j] - b[j]) * (a[j] - b[j])
            + (a[j + 1] - b[j + 1]) * (a[j + 1] - b[j + 1])
            + (a[j + 2] - b[j + 2]) * (a[j + 2] - b[j + 2])
            + (a[j + 3] - b[j + 3]) * (a[j + 3] - b[j + 3]);
    }
    for j in blocks * 4..n {
        total += (a[j] - b[j]) * (a[j] - b[j]);
    }
    total
}

/// `_euclidean_distances(..., squared=True)`: `(-2·x·y + ‖x‖²) + ‖y‖²`, sin negativos.
fn expanded(x: &[f64], x_norm: f64, y: &[f64], y_norm: f64) -> f64 {
    ((-2.0 * dot(x, y) + x_norm) + y_norm).max(0.0)
}

fn sum(values: &[f64]) -> f64 {
    let mut total = 0.0;
    for v in values {
        total += v;
    }
    total
}

/// Media por columna recorriendo las filas (reducción de NumPy sobre el eje 0).
fn column_means(data: &[Vec<f64>]) -> Vec<f64> {
    let n = data.len() as f64;
    let mut means = vec![0.0; data[0].len()];
    for row in data {
        for (m, v) in means.iter_mut().zip(row) {
            *m += v;
        }
    }
    means.iter().map(|s| s / n).collect()
}

/// `np.mean(np.var(X, axis=0)) * 1e-4`.
fn tolerance(data: &[Vec<f64>]) -> f64 {
    let n = data.len() as f64;
    let means = column_means(data);
    let mut squares = vec![0.0; means.len()];
    for row in data {
        for ((s, v), m) in squares.iter_mut().zip(row).zip(&means) {
            *s += (v - m) * (v - m);
        }
    }
    let variances: Vec<f64> = squares.iter().map(|s| s / n).collect();
    sum(&variances) / variances.len() as f64 * 1e-4
}

fn plus_plus(data: &[Vec<f64>], norms: &[f64], k: usize, rng: &mut Mt19937) -> Vec<Vec<f64>> {
    let n = data.len();
    let trials = 2 + (k as f64).ln() as usize;
    // choice(n, p=pesos/suma): CDF normalizada y búsqueda a la derecha.
    let p = 1.0 / n as f64;
    let mut cdf = Vec::with_capacity(n);
    let mut running = 0.0;
    for _ in 0..n {
        running += p;
        cdf.push(running);
    }
    let last = cdf[n - 1];
    for value in &mut cdf {
        *value /= last;
    }
    let u = rng.next_f64();
    let first = cdf.partition_point(|c| *c <= u).min(n - 1);

    let mut centers = vec![data[first].clone()];
    let center_norm = dot(&data[first], &data[first]);
    let mut closest: Vec<f64> = data
        .iter()
        .zip(norms)
        .map(|(x, nx)| expanded(&data[first], center_norm, x, *nx))
        .collect();
    let mut potential = sum(&closest);
    for _ in 1..k {
        let draws: Vec<f64> = (0..trials).map(|_| rng.next_f64() * potential).collect();
        let mut cumulative = Vec::with_capacity(n);
        let mut running = 0.0;
        for d in &closest {
            running += d;
            cumulative.push(running);
        }
        let mut best: Option<(f64, usize, Vec<f64>)> = None;
        for draw in draws {
            let candidate = cumulative.partition_point(|c| *c < draw).min(n - 1);
            let candidate_norm = dot(&data[candidate], &data[candidate]);
            let distances: Vec<f64> = data
                .iter()
                .zip(norms)
                .zip(&closest)
                .map(|((x, nx), c)| expanded(&data[candidate], candidate_norm, x, *nx).min(*c))
                .collect();
            let total = sum(&distances);
            if best.as_ref().is_none_or(|(p, _, _)| total < *p) {
                best = Some((total, candidate, distances));
            }
        }
        let (total, candidate, distances) = best.expect("at least one candidate");
        centers.push(data[candidate].clone());
        closest = distances;
        potential = total;
    }
    centers
}

/// Etiquetas por `‖c‖² - 2·x·c` (el término `‖x‖²` no cambia el mínimo).
fn assign(data: &[Vec<f64>], centers: &[Vec<f64>]) -> Vec<usize> {
    let norms: Vec<f64> = centers.iter().map(|c| dot(c, c)).collect();
    data.iter()
        .map(|x| {
            let mut label = 0;
            let mut best = norms[0] + -2.0 * dot(x, &centers[0]);
            for (j, center) in centers.iter().enumerate().skip(1) {
                let distance = norms[j] + -2.0 * dot(x, center);
                if distance < best {
                    best = distance;
                    label = j;
                }
            }
            label
        })
        .collect()
}

fn lloyd(data: &[Vec<f64>], mut centers: Vec<Vec<f64>>, tolerance: f64) -> Clustering {
    let k = centers.len();
    let dims = data[0].len();
    let mut labels_old: Vec<usize> = vec![usize::MAX; data.len()];
    let mut labels = labels_old.clone();
    let mut strict = false;
    for _ in 0..300 {
        labels = assign(data, &centers);
        let mut sums = vec![vec![0.0; dims]; k];
        let mut weights = vec![0.0; k];
        for (x, &label) in data.iter().zip(&labels) {
            weights[label] += 1.0;
            for (s, v) in sums[label].iter_mut().zip(x) {
                *s += v * 1.0;
            }
        }
        let empty: Vec<usize> = (0..k).filter(|&j| weights[j] == 0.0).collect();
        if !empty.is_empty() {
            let distances: Vec<f64> = data
                .iter()
                .zip(&labels)
                .map(|(x, &l)| squared_unrolled(x, &centers[l]))
                .collect();
            let mut far: Vec<usize> = (0..data.len()).collect();
            far.sort_by(|a, b| distances[*b].total_cmp(&distances[*a]).then(a.cmp(b)));
            for &cluster in &empty {
                // Solo dona un punto desde un grupo con >1 asignado, garantizando que
                // ningun centro quede con peso 0 ni NaN al normalizar sums / weights.
                let Some(&point) = far.iter().find(|&&p| weights[labels[p]] > 1.0) else {
                    break;
                };
                let old = labels[point];
                for d in 0..dims {
                    sums[old][d] -= data[point][d];
                    sums[cluster][d] = data[point][d];
                }
                labels[point] = cluster;
                weights[cluster] = 1.0;
                weights[old] -= 1.0;
            }
        }
        for (center, weight) in sums.iter_mut().zip(&weights) {
            if *weight > 0.0 {
                let alpha = 1.0 / weight;
                for v in center.iter_mut() {
                    *v *= alpha;
                }
            }
        }
        let shift: f64 = {
            let shifts: Vec<f64> = centers
                .iter()
                .zip(&sums)
                .map(|(a, b)| squared_unrolled(a, b).sqrt())
                .collect();
            let squares: Vec<f64> = shifts.iter().map(|s| s * s).collect();
            crate::numeric::pairwise_sum(&squares)
        };
        centers = sums;
        if labels == labels_old && empty.is_empty() {
            strict = true;
            break;
        }
        if shift <= tolerance {
            break;
        }
        labels_old.clone_from(&labels);
    }
    if !strict {
        labels = assign(data, &centers);
    }
    let mut inertia = 0.0;
    for (x, &label) in data.iter().zip(&labels) {
        inertia += squared_unrolled(x, &centers[label]) * 1.0;
    }
    Clustering {
        labels,
        centers,
        inertia,
    }
}

/// Mismo agrupamiento con otras etiquetas (`_is_same_clustering`).
fn same_clustering(a: &[usize], b: &[usize], k: usize) -> bool {
    let mut mapping = vec![usize::MAX; k];
    for (&x, &y) in a.iter().zip(b) {
        if mapping[x] == usize::MAX {
            mapping[x] = y;
        } else if mapping[x] != y {
            return false;
        }
    }
    true
}

/// `KMeans(n_clusters=k, n_init=n_init, random_state=seed).fit(data)`.
pub fn fit(
    data: &[Vec<f64>],
    k: usize,
    n_init: usize,
    seed: u32,
) -> Result<Clustering, ModelError> {
    if data.is_empty() {
        return Err(ModelError("kmeans requires at least one row".into()));
    }
    let dims = data[0].len();
    if dims == 0 || data.iter().any(|row| row.len() != dims) {
        return Err(ModelError("kmeans rows have different lengths".into()));
    }
    if data.iter().flatten().any(|value| !value.is_finite()) {
        return Err(ModelError("kmeans data contains non-finite values".into()));
    }
    if k == 0 || k > data.len() {
        return Err(ModelError("kmeans requires 1 <= k <= rows".into()));
    }
    let tolerance = tolerance(data);
    let means = column_means(data);
    let centered: Vec<Vec<f64>> = data
        .iter()
        .map(|row| row.iter().zip(&means).map(|(v, m)| v - m).collect())
        .collect();
    let norms: Vec<f64> = centered.iter().map(|x| dot(x, x)).collect();
    let mut rng = Mt19937::new(seed);
    let mut best: Option<Clustering> = None;
    for _ in 0..n_init.max(1) {
        let initial = plus_plus(&centered, &norms, k, &mut rng);
        let run = lloyd(&centered, initial, tolerance);
        let better = match &best {
            None => true,
            Some(b) => run.inertia < b.inertia && !same_clustering(&run.labels, &b.labels, k),
        };
        if better {
            best = Some(run);
        }
    }
    let mut best = best.expect("at least one run");
    for center in &mut best.centers {
        for (v, m) in center.iter_mut().zip(&means) {
            *v += m;
        }
    }
    Ok(best)
}

/// `silhouette_score` euclidiano; exige al menos dos grupos con dos o más puntos.
pub fn silhouette(data: &[Vec<f64>], labels: &[usize]) -> Result<f64, ModelError> {
    if data.is_empty() || data.len() != labels.len() {
        return Err(ModelError("silhouette requires one label per row".into()));
    }
    let n = data.len();
    let dims = data[0].len();
    if dims == 0 || data.iter().any(|row| row.len() != dims) {
        return Err(ModelError("silhouette rows have different lengths".into()));
    }
    let k = labels.iter().copied().max().map_or(0, |m| m + 1);
    if k < 2 || k >= data.len() {
        return Err(ModelError(
            "silhouette requires at least two clusters".into(),
        ));
    }
    let mut frequencies = vec![0.0f64; k];
    for &l in labels {
        frequencies[l] += 1.0;
    }
    if frequencies.iter().any(|count| *count < 1.0) || data.iter().flatten().any(|v| !v.is_finite())
    {
        return Err(ModelError(
            "silhouette requires finite rows and non-empty clusters".into(),
        ));
    }
    let norms: Vec<f64> = data.iter().map(|x| dot(x, x)).collect();
    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        let mut clusters = vec![0.0; k];
        for j in 0..n {
            let distance = if i == j {
                0.0
            } else {
                expanded(&data[i], norms[i], &data[j], norms[j]).sqrt()
            };
            clusters[labels[j]] += distance;
        }
        let own = labels[i];
        // Un grupo de un solo punto aporta 0, como en scikit-learn.
        let intra = if frequencies[own] > 1.0 {
            clusters[own] / (frequencies[own] - 1.0)
        } else {
            0.0
        };
        clusters[own] = f64::INFINITY;
        let inter = clusters
            .iter()
            .zip(&frequencies)
            .map(|(c, f)| c / f)
            .fold(f64::INFINITY, f64::min);
        let value = (inter - intra) / intra.max(inter);
        samples.push(if value.is_nan() { 0.0 } else { value });
    }
    Ok(crate::numeric::mean(&samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_matches_numpy_random_state() {
        // np.random.RandomState(42).random_sample(3)
        let mut rng = Mt19937::new(42);
        assert_eq!(rng.next_f64(), 0.3745401188473625);
        assert_eq!(rng.next_f64(), 0.9507143064099162);
        assert_eq!(rng.next_f64(), 0.7319939418114051);
    }

    fn blobs() -> Vec<Vec<f64>> {
        let mut rng = Mt19937::new(7);
        let mut data = Vec::new();
        for center in [[0.0, 0.0], [5.0, 5.0], [0.0, 5.0]] {
            for _ in 0..20 {
                data.push(vec![
                    center[0] + rng.next_f64() * 0.5,
                    center[1] + rng.next_f64() * 0.5,
                ]);
            }
        }
        data
    }

    #[test]
    fn finds_separated_groups_and_is_deterministic() {
        let data = blobs();
        let first = fit(&data, 3, 10, 42).unwrap();
        let second = fit(&data, 3, 10, 42).unwrap();
        assert_eq!(first.labels, second.labels);
        for block in first.labels.chunks(20) {
            assert!(block.iter().all(|l| *l == block[0]));
        }
        assert!(silhouette(&data, &first.labels).unwrap() > 0.8);
    }

    #[test]
    fn silhouette_prefers_the_true_number_of_groups() {
        let data = blobs();
        let scores: Vec<f64> = (2..=4)
            .map(|k| silhouette(&data, &fit(&data, k, 10, 42).unwrap().labels).unwrap())
            .collect();
        assert!(scores[1] > scores[0] && scores[1] > scores[2], "{scores:?}");
    }

    #[test]
    fn invalid_inputs_are_errors_instead_of_panics() {
        assert!(fit(&[], 1, 10, 42).is_err());
        let data = blobs();
        assert!(fit(&data, 0, 10, 42).is_err());
        assert!(fit(&data, data.len() + 1, 10, 42).is_err());
        assert!(fit(&[vec![0.0, 1.0], vec![0.0]], 1, 10, 42).is_err());
        assert!(fit(&[vec![0.0, f64::NAN]], 1, 10, 42).is_err());
        assert!(silhouette(&[], &[]).is_err());
        assert!(silhouette(&data, &[0]).is_err());
        assert!(silhouette(&data, &vec![0; data.len()]).is_err());
        assert!(silhouette(&data, &vec![0; data.len() - 1]).is_err());
    }

    #[test]
    fn empty_cluster_rescue_updates_labels_and_keeps_all_centers_finite() {
        // Dos puntos muy cercanos y uno aislado pueden vaciar un cluster en la primera asignacion;
        // al rescatar k == data.len(), ningun centro puede quedar vacio ni producir NaN.
        let data = vec![vec![0.0, 0.0], vec![10.0, 10.0], vec![10.001, 10.001]];
        let result = fit(&data, 3, 25, 1).unwrap();
        assert_eq!(result.centers.len(), 3);
        assert!(result
            .centers
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
        let mut unique = result.labels.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 3);
    }
}

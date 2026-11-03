//! Índice de salud con lógica difusa (inferencia Sugeno de orden cero).
//!
//! Entrada por variable: la desviación respecto al rango ideal, medida en tolerancias. Conjuntos: «ideal»,
//! «aceptable», «desviada» y «crítica». Cada conjunto aporta un valor de salud y el resultado de cada variable es el
//! promedio ponderado por su grado de pertenencia. El índice global pondera las variables por su importancia
//! agronómica y aplica una regla de peor caso: una variable crítica limita la salud total.

use crate::domain::Parameter;
use crate::format::py_round;
use crate::numeric::py_sum;
use crate::schema::response::{Health, Ordered};

/// Peso agronómico de cada variable.
pub fn weight(parameter: Parameter) -> f64 {
    match parameter {
        Parameter::Ph => 1.3,
        Parameter::Temperature => 1.2,
        Parameter::SoilMoisture => 1.2,
        Parameter::Tds => 1.1,
        Parameter::Humidity => 0.9,
        Parameter::Brightness => 0.8,
    }
}

/// Valor de salud que representa cada conjunto (consecuente de la regla).
const OUTPUTS: [f64; 4] = [100.0, 75.0, 45.0, 10.0];

/// Conjuntos difusos como trapecios `(a, b, c, d)`; el último está abierto a la derecha.
const SETS: [(f64, f64, f64, f64); 4] = [
    (-1.0, 0.0, 0.0, 0.25),
    (0.0, 0.25, 0.4, 0.7),
    (0.4, 0.7, 0.9, 1.2),
    (0.9, 1.2, 1e9, 1e9),
];

const LEVELS: [(f64, &str, &str); 5] = [
    (85.0, "EXCELLENT", "Excelente"),
    (70.0, "GOOD", "Saludable"),
    (50.0, "FAIR", "Aceptable"),
    (30.0, "POOR", "En riesgo"),
    (0.0, "CRITICAL", "Crítico"),
];

/// Pertenencia trapezoidal sin ramificaciones: `max(0, min(subida, 1, bajada))`.
///
/// Con flancos verticales (`b == a` o `d == c`) el flanco no limita. El resultado es idéntico, bit a bit, a la versión
/// con condicionales del servicio en Python: dentro de cada tramo gana la misma expresión.
pub fn trapezoid(x: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let rise = if b > a {
        (x - a) / (b - a)
    } else {
        f64::INFINITY
    };
    let fall = if d > c {
        (d - x) / (d - c)
    } else {
        f64::INFINITY
    };
    let inside = rise.min(fall).min(1.0);
    // Fuera del soporte una de las dos rectas es negativa; los bordes exactos dan 0 en ambas versiones.
    let outside = (x < a) | (x > d);
    if outside {
        0.0
    } else {
        inside.max(0.0)
    }
}

/// Grados de pertenencia: ideal, aceptable, desviada y crítica.
pub fn memberships(deviation: f64) -> [f64; 4] {
    let d = deviation.max(0.0);
    SETS.map(|(a, b, c, e)| trapezoid(d, a, b, c, e))
}

pub fn parameter_health(deviation: f64) -> f64 {
    let degrees = memberships(deviation);
    let total = py_sum(degrees);
    let weighted = py_sum(
        OUTPUTS
            .iter()
            .zip(degrees)
            .map(|(output, degree)| output * degree),
    );
    if total == 0.0 {
        OUTPUTS[3]
    } else {
        weighted / total
    }
}

/// Índice de 0 a 100 a partir de las desviaciones, en el orden del diagnóstico.
pub fn health_index(deviations: &[(Parameter, f64)]) -> Health {
    if deviations.is_empty() {
        return Health {
            index: 0.0,
            level: "UNKNOWN",
            label: "Sin datos",
            by_parameter: Ordered::default(),
        };
    }
    let scores: Vec<(Parameter, f64)> = deviations
        .iter()
        .map(|&(p, d)| (p, parameter_health(d)))
        .collect();
    // `sum()` de Python 3.12+ compensa el redondeo: hay que sumar igual para obtener los mismos bits.
    let numerator = py_sum(scores.iter().map(|&(p, score)| score * weight(p)));
    let denominator = py_sum(scores.iter().map(|&(p, _)| weight(p)));
    let weighted = numerator / denominator;
    let worst_critical = deviations
        .iter()
        .map(|&(_, d)| memberships(d)[3])
        .fold(f64::NEG_INFINITY, f64::max);
    // Regla de peor caso: una variable plenamente crítica deja la salud en «En riesgo» como máximo.
    let cap = 100.0 - 60.0 * worst_critical;
    let index = py_round(weighted.min(cap), 1);
    let (_, level, label) = LEVELS
        .iter()
        .copied()
        .find(|(threshold, _, _)| index >= *threshold)
        .unwrap_or(LEVELS[4]);
    let by_parameter = scores
        .iter()
        .map(|&(p, s)| (p.name().to_string(), py_round(s, 1)))
        .collect();
    Health {
        index,
        level,
        label,
        by_parameter: Ordered(by_parameter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branched(x: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
        if x < a || x > d {
            return 0.0;
        }
        if b <= x && x <= c {
            return 1.0;
        }
        if x < b {
            return if b > a { (x - a) / (b - a) } else { 1.0 };
        }
        if d > c {
            (d - x) / (d - c)
        } else {
            1.0
        }
    }

    #[test]
    fn branchless_trapezoid_matches_the_branched_version() {
        let mut points: Vec<f64> = vec![-1.0, 0.0, 0.25, 0.4, 0.7, 0.9, 1.2, 1e9, 2e9];
        let mut x = -1.5;
        while x < 3.0 {
            points.push(x);
            x += 0.0013;
        }
        for &x in &points {
            let neighbours = [x, x.next_up(), x.next_down()];
            for probe in neighbours {
                for &(a, b, c, d) in &SETS {
                    let fast = trapezoid(probe, a, b, c, d).to_bits();
                    assert_eq!(fast, branched(probe, a, b, c, d).to_bits(), "x={probe}");
                }
            }
        }
    }

    #[test]
    fn health_decreases_with_deviation() {
        let mut previous = f64::INFINITY;
        let mut d = 0.0;
        while d <= 3.0 {
            let current = parameter_health(d);
            assert!(current <= previous + 1e-12, "d={d}");
            previous = current;
            d += 0.01;
        }
    }

    #[test]
    fn a_critical_variable_caps_the_index() {
        let health = health_index(&[
            (Parameter::Ph, 2.0),
            (Parameter::Temperature, 0.0),
            (Parameter::Humidity, 0.0),
        ]);
        assert!(health.index <= 40.0);
        let ideal = health_index(&[(Parameter::Ph, 0.0)]);
        assert_eq!((ideal.index, ideal.level), (100.0, "EXCELLENT"));
        assert_eq!(health_index(&[]).level, "UNKNOWN");
    }
}

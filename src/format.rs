//! Formato de números y textos idéntico al del servicio en Python.
//!
//! Rust y Python redondean igual al formatear con precisión fija: expansión decimal exacta del valor binario y,
//! en los empates, al par. Por eso `round(x, n)` de Python equivale a formatear con `n` decimales y volver a leer.

/// `round(value, digits)` de Python.
pub fn py_round(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}").parse().unwrap_or(value)
}

/// `round(value)` de Python para un número que se usa como entero (mitad al par).
pub fn py_round_int(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// `f"{value:.1f}".rstrip("0").rstrip(".")`: un decimal, sin ceros ni punto sobrantes.
pub fn short(value: f64) -> String {
    let text = format!("{value:.1}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `f"{value:.1f}"`.
pub fn one_decimal(value: f64) -> String {
    format!("{value:.1}")
}

/// `f"{value:.0f}"`.
pub fn no_decimals(value: f64) -> String {
    format!("{value:.0}")
}

/// `f"{value:.0%}"`: Python multiplica por 100 en coma flotante y formatea sin decimales.
pub fn percent(value: f64) -> String {
    format!("{:.0}%", value * 100.0)
}

/// `f"{value:,}".replace(",", ".")`: miles separados con punto.
pub fn thousands(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    if value < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

/// `text[:1].upper() + text[1:]`.
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_like_python() {
        assert_eq!(py_round(2.675, 2), 2.67);
        assert_eq!(py_round(0.125, 2), 0.12);
        assert_eq!(py_round(0.375, 2), 0.38);
        assert_eq!(py_round(72.25, 1), 72.2);
        assert_eq!(py_round(-0.0001, 2).to_bits(), (-0.0f64).to_bits());
        assert_eq!(py_round_int(2.5), 2);
        assert_eq!(py_round_int(3.5), 4);
    }

    #[test]
    fn formats_like_python() {
        assert_eq!(no_decimals(0.5), "0");
        assert_eq!(no_decimals(1.5), "2");
        assert_eq!(no_decimals(72.5), "72");
        assert_eq!(one_decimal(0.25), "0.2");
        assert_eq!(short(21.0), "21");
        assert_eq!(short(0.04), "0");
        assert_eq!(short(-0.04), "-0");
        assert_eq!(short(5.55), "5.5");
        assert_eq!(short(1400.0), "1400");
        assert_eq!(percent(0.735), "74%");
        assert_eq!(percent(0.705), "70%");
        assert_eq!(thousands(12_345), "12.345");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000_000), "1.000.000");
        assert_eq!(capitalize("soil moisture"), "Soil moisture");
        assert_eq!(capitalize("pH"), "PH");
    }
}

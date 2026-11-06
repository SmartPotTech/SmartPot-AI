//! Verificación de los modelos que publica SmartPot-Trainer: firma Ed25519 del manifiesto y hash BLAKE2b de cada
//! archivo.
//!
//! El Trainer firma el JSON canónico de Python (`json.dumps(..., sort_keys=True, separators=(",", ":"))`, con
//! caracteres no ASCII escapados) sin el campo `signature_hex`. Aquí se reconstruyen exactamente esos bytes. Con una
//! clave de confianza (`TRUSTED_MODEL_PUBLIC_KEY_HEX`) solo se acepta lo firmado con ella: la clave que trae el propio
//! manifiesto no prueba nada.

use std::fmt::Write as _;

use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::Value;

type Blake2b256 = Blake2b<U32>;

/// BLAKE2b de 256 bits en hexadecimal (`hashlib.blake2b(digest_size=32)`).
pub fn blake2b_hex(bytes: &[u8]) -> String {
    hex::encode(Blake2b256::digest(bytes))
}

/// Lee una clave pública Ed25519 en hexadecimal (32 bytes).
pub fn parse_public_key(text: &str) -> Result<VerifyingKey, String> {
    let bytes = hex::decode(text.trim()).map_err(|_| "public key is not valid hex".to_string())?;
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "public key must be 32 bytes".to_string())?;
    VerifyingKey::from_bytes(&array)
        .map_err(|_| "public key is not a valid curve point".to_string())
}

/// `repr(float)` de Python: dígitos mínimos, notación fija entre 1e-4 y 1e16 y exponente con signo y dos cifras.
pub fn python_float(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("scientific notation");
    let exponent: i32 = exponent.parse().expect("exponente");
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let sign = if negative { "-" } else { "" };
    if (-4..16).contains(&exponent) {
        let point = exponent + 1;
        let body = if point <= 0 {
            format!("0.{}{}", "0".repeat((-point) as usize), digits)
        } else if point as usize >= digits.len() {
            format!("{}{}.0", digits, "0".repeat(point as usize - digits.len()))
        } else {
            format!(
                "{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            )
        };
        format!("{sign}{body}")
    } else {
        let tail = if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        let exp_sign = if exponent < 0 { '-' } else { '+' };
        format!(
            "{sign}{}{tail}e{exp_sign}{:02}",
            &digits[..1],
            exponent.abs()
        )
    }
}

fn python_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if c < ' ' || c == '\u{007f}' || !c.is_ascii() => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// JSON canónico de Python con claves ordenadas y sin espacios.
pub fn python_canonical(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                let _ = write!(out, "{i}");
            } else if let Some(u) = n.as_u64() {
                let _ = write!(out, "{u}");
            } else {
                out.push_str(&python_float(n.as_f64().unwrap_or(0.0)));
            }
        }
        Value::String(s) => python_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                python_string(key, out);
                out.push(':');
                write_canonical(&map[key.as_str()], out);
            }
            out.push('}');
        }
    }
}

/// Resultado de verificar un manifiesto.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Firma válida con la clave de confianza y hash correcto.
    Signed,
    /// Sin clave de confianza: solo se comprobó el hash del archivo.
    HashOnly,
}

/// Comprueba el manifiesto de una especie y el hash del archivo que se va a cargar.
pub fn verify(
    manifest_bytes: &[u8],
    artifact: &str,
    artifact_bytes: &[u8],
    trusted: Option<&VerifyingKey>,
) -> Result<Verdict, String> {
    let manifest: Value = serde_json::from_slice(manifest_bytes)
        .map_err(|_| "manifest is not valid JSON".to_string())?;
    let Value::Object(map) = &manifest else {
        return Err("manifest is not a JSON object".into());
    };
    let expected = map
        .get("artifacts")
        .and_then(|a| a.get(artifact))
        .and_then(|meta| meta.get("blake2b"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("manifest does not declare {artifact}"))?;
    let actual = blake2b_hex(artifact_bytes);
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(format!("BLAKE2b hash for {artifact} does not match"));
    }
    let Some(key) = trusted else {
        return Ok(Verdict::HashOnly);
    };
    let signature_hex = map
        .get("signature_hex")
        .and_then(Value::as_str)
        .ok_or("manifest is missing signature")?;
    let signature_bytes: [u8; 64] = hex::decode(signature_hex)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("signature is not 64 hex bytes")?;
    let mut unsigned = map.clone();
    unsigned.remove("signature_hex");
    let payload = python_canonical(&Value::Object(unsigned));
    key.verify_strict(payload.as_bytes(), &Signature::from_bytes(&signature_bytes))
        .map_err(|_| "Ed25519 signature does not match trusted public key".to_string())?;
    Ok(Verdict::Signed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn floats_match_python_repr() {
        for (value, expected) in [
            (1730582400.123456, "1730582400.123456"),
            (1730582400.0, "1730582400.0"),
            (0.1, "0.1"),
            (1e16, "1e+16"),
            (1.5e-5, "1.5e-05"),
            (0.0001, "0.0001"),
            (123456789012345.6, "123456789012345.6"),
            (-2.5, "-2.5"),
            (1e22, "1e+22"),
        ] {
            assert_eq!(python_float(value), expected);
        }
    }

    #[test]
    fn canonical_json_sorts_keys_and_escapes_like_python() {
        // Las secuencias de escape se arman al ejecutar, con la barra invertida como carácter.
        let escape = |unit: u32| format!("{}u{unit:04x}", '\\');
        let value = json!({"b": 1, "a": {"z": [true, null], "y": "Naïve baseline"}, "c": 0.5});
        let expected = format!(
            r#"{{"a":{{"y":"Na{}ve baseline","z":[true,null]}},"b":1,"c":0.5}}"#,
            escape(0xef)
        );
        assert_eq!(python_canonical(&value), expected);
        let plant = char::from_u32(0x1F331).unwrap().to_string();
        let escaped = format!("\"{}{}\"", escape(0xd83c), escape(0xdf31));
        assert_eq!(python_canonical(&json!(plant)), escaped);
        assert_eq!(
            python_canonical(&json!("\u{007f}")),
            format!("\"{}u007f\"", '\\')
        );
    }

    #[test]
    fn blake2b_256_matches_hashlib() {
        // hashlib.blake2b(b"", digest_size=32).hexdigest()
        assert_eq!(
            blake2b_hex(b""),
            "0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8"
        );
    }
}

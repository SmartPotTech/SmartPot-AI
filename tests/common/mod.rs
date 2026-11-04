//! Utilidades compartidas de las pruebas de integración.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use smartpot_ia::config::Settings;
use smartpot_ia::engine::models::BaseModels;
use smartpot_ia::state::AppState;
use tower::ServiceExt;

pub const TOKEN: &str = "parity-token-for-local-tests-only";

pub fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("fixture JSON")
}

pub fn state_with(pairs: &[(&str, &str)]) -> AppState {
    let mut map: HashMap<String, String> = HashMap::from([
        ("SMARTPOT_AI_TOKEN".to_string(), TOKEN.to_string()),
        ("DATA_DIR".to_string(), String::new()),
    ]);
    for (k, v) in pairs {
        map.insert((*k).to_string(), (*v).to_string());
    }
    let settings = Settings::from_lookup(|name| map.get(name).cloned()).expect("test settings");
    AppState::new(settings, BaseModels::embedded().expect("base models")).expect("state")
}

pub fn state() -> AppState {
    state_with(&[])
}

pub async fn call(
    app: &Router,
    method: &str,
    path: &str,
    body: Option<&Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(body).unwrap()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

/// Compara dos JSON: textos y estructura exactos, números con tolerancia de redondeo binario.
pub fn differences(path: &str, expected: &Value, actual: &Value, out: &mut Vec<String>) {
    match (expected, actual) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            if (a - b).abs() > 1e-9 * a.abs().max(1.0) {
                out.push(format!("{path}: {a} ≠ {b}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path}: {} items ≠ {}", a.len(), b.len()));
                return;
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                differences(&format!("{path}[{i}]"), x, y, out);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                match (a.get(key), b.get(key)) {
                    (Some(x), Some(y)) => differences(&format!("{path}.{key}"), x, y, out),
                    _ => out.push(format!("{path}.{key}: missing on one side")),
                }
            }
        }
        (a, b) if a == b => {}
        (a, b) => out.push(format!("{path}: {a} ≠ {b}")),
    }
}

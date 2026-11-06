//! Casos de uso compartidos por REST y gRPC: el mismo JSON entra y el mismo JSON sale por ambos transportes.

pub mod insights;

use chrono::{DateTime, Timelike, Utc};
use serde_json::{json, Value};

use crate::engine::fleet;
use crate::schema::{parse_fleet, parse_insight, FieldError};
use crate::state::AppState;

/// Error de una evaluación: datos inválidos (422 en REST, `INVALID_ARGUMENT` en gRPC).
#[derive(Debug, Clone)]
pub struct Invalid(pub Vec<FieldError>);

impl Invalid {
    pub fn body(&self) -> Value {
        json!({"detail": "Invalid data", "errors": self.0})
    }

    pub fn json() -> Self {
        Invalid(vec![FieldError {
            field: String::new(),
            message: "Request body is not valid JSON".into(),
        }])
    }
}

fn now() -> f64 {
    chrono::Utc::now().timestamp_micros() as f64 / 1e6
}

/// Hora local de una marca temporal; `None` si el instante no es representable.
pub fn local_hour(micros: i64, timezone: chrono_tz::Tz) -> Option<u8> {
    DateTime::<Utc>::from_timestamp_micros(micros)
        .map(|moment| moment.with_timezone(&timezone).hour() as u8)
}

/// `POST /v1/insights` (y `EvaluateInsight` por gRPC).
pub fn insight(state: &AppState, body: &[u8]) -> Result<Vec<u8>, Invalid> {
    let value: Value = serde_json::from_slice(body).map_err(|_| Invalid::json())?;
    let request = parse_insight(&value).map_err(Invalid)?;
    let response = insights::analyze(state, &request, now());
    Ok(serde_json::to_vec(&response).expect("the response always serializes"))
}

/// `POST /v1/fleet` (y `AnalyzeFleet` por gRPC).
pub fn fleet(state: &AppState, body: &[u8]) -> Result<Vec<u8>, Invalid> {
    let value: Value = serde_json::from_slice(body).map_err(|_| Invalid::json())?;
    let request = parse_fleet(&value).map_err(Invalid)?;
    let hour = request.local_hour.or_else(|| {
        request
            .crops
            .iter()
            .filter_map(|crop| crop.measures.as_ref().and_then(|m| m.measured_at))
            .max()
            .and_then(|micros| local_hour(micros, state.settings.timezone))
    });
    Ok(
        serde_json::to_vec(&fleet::analyze(&request, hour))
            .expect("the response always serializes"),
    )
}

//! API REST (HTTP/1.0, HTTP/1.1 y HTTP/2 sin TLS): las mismas rutas, códigos y mensajes del servicio en Python.
//!
//! `/health` es público; todo `/v1/*` exige el token de servicio. Las rutas de aprendizaje se reenvían a
//! SmartPot-Trainer; la recarga de modelos es local.

use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::Router;
use reqwest::Method;
use serde_json::{json, Value};
use tower_http::trace::TraceLayer;

use crate::auth::{bearer_matches, INVALID_TOKEN};
use crate::domain::{profile, PROFILES};
use crate::format::py_round;
use crate::learning::trainer::Forwarded;
use crate::schema::response::{CropProfileOut, Ordered};
use crate::services::{self, Invalid};
use crate::state::AppState;

const BODY_LIMIT: usize = 4 * 1024 * 1024;
const TRAINER_TIMEOUT: Duration = Duration::from_secs(10);
const TRAINING_TIMEOUT: Duration = Duration::from_secs(120);

fn json_response(status: StatusCode, body: Vec<u8>) -> Response {
    let mut response = (status, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

fn json_value(status: StatusCode, value: &Value) -> Response {
    json_response(
        status,
        serde_json::to_vec(value).expect("JSON serializable"),
    )
}

fn detail(status: StatusCode, message: impl Into<String>) -> Response {
    json_value(status, &json!({ "detail": message.into() }))
}

fn invalid(error: &Invalid) -> Response {
    json_value(StatusCode::UNPROCESSABLE_ENTITY, &error.body())
}

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/insights", post(insights))
        .route("/fleet", post(fleet))
        .route("/crop-profiles", get(crop_profiles))
        .route("/profiles", get(crop_profiles))
        .route("/profiles/{crop_type}", get(crop_profile))
        .route("/models", get(models))
        .route("/learning/reload", post(reload))
        .route("/learning/readings", post(readings))
        .route("/learning/status", get(status))
        .route("/learning/train", post(train))
        .route("/learning/crops/{crop_id}", delete(forget))
        .route("/learning/prune", post(prune))
        // Las rutas inexistentes y los métodos incorrectos también exigen el token antes de
        // revelar si existen, igual que gRPC: `layer`, a diferencia de `route_layer`, cubre los fallbacks.
        .fallback(|| async { detail(StatusCode::NOT_FOUND, "Not Found") })
        .method_not_allowed_fallback(|| async {
            detail(StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed")
        })
        .layer(middleware::from_fn_with_state(state.clone(), require_token));
    Router::new()
        .route("/health", get(health))
        .nest("/v1", protected)
        .fallback(|| async { detail(StatusCode::NOT_FOUND, "Not Found") })
        .method_not_allowed_fallback(|| async {
            detail(StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed")
        })
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn require_token(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if bearer_matches(header, &state.settings.token) {
        return next.run(request).await;
    }
    let mut response = detail(StatusCode::UNAUTHORIZED, INVALID_TOKEN);
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

async fn health(State(state): State<AppState>) -> Response {
    let learning = if state.trainer.is_some() {
        "TRAINER"
    } else if state.learned.directory().is_some() {
        "LOCAL"
    } else {
        "OFF"
    };
    json_value(
        StatusCode::OK,
        &json!({
            "status": "UP",
            "service": "smartpot-ia-rust",
            "version": crate::version(),
            "models": "READY",
            "learning": learning,
            "learnedCropTypes": state.learned.crop_types(),
            "signedModels": state.learned.is_signed(),
        }),
    )
}

async fn insights(State(state): State<AppState>, body: Bytes) -> Response {
    match services::insight(&state, &body) {
        Ok(bytes) => json_response(StatusCode::OK, bytes),
        Err(error) => invalid(&error),
    }
}

async fn fleet(State(state): State<AppState>, body: Bytes) -> Response {
    match services::fleet(&state, &body) {
        Ok(bytes) => json_response(StatusCode::OK, bytes),
        Err(error) => invalid(&error),
    }
}

async fn crop_profiles() -> Response {
    let profiles: Vec<CropProfileOut> = PROFILES.iter().map(CropProfileOut::from_profile).collect();
    json_response(
        StatusCode::OK,
        serde_json::to_vec(&profiles).expect("JSON serializable"),
    )
}

async fn crop_profile(Path(crop_type): Path<String>) -> Response {
    match profile(&crop_type) {
        Some(found) => json_response(
            StatusCode::OK,
            serde_json::to_vec(&CropProfileOut::from_profile(found)).expect("JSON serializable"),
        ),
        None => detail(
            StatusCode::NOT_FOUND,
            format!("Unknown crop type: {crop_type}"),
        ),
    }
}

async fn models(State(state): State<AppState>) -> Response {
    let metrics = Ordered(
        state
            .base
            .metrics
            .iter()
            .map(|(name, value)| (name.clone(), py_round(*value, 4)))
            .collect(),
    );
    json_response(
        StatusCode::OK,
        serde_json::to_vec(&metrics).expect("JSON serializable"),
    )
}

/// Especie opcional de los cuerpos `{"cropType": ...}` (recarga y entrenamiento).
fn crop_type_of(body: &[u8]) -> Result<Option<String>, Box<Response>> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let value: Value =
        serde_json::from_slice(body).map_err(|_| Box::new(invalid(&Invalid::json())))?;
    let raw = value
        .get("cropType")
        .or_else(|| value.get("crop_type"))
        .filter(|v| !v.is_null());
    match raw {
        None => Ok(None),
        Some(Value::String(text)) => match profile(text) {
            Some(found) => Ok(Some(found.kind.to_string())),
            None => Err(Box::new(invalid(&Invalid(vec![
                crate::schema::FieldError {
                    field: "cropType".into(),
                    message: format!("Unknown crop type: {text}"),
                },
            ])))),
        },
        Some(_) => Err(Box::new(invalid(&Invalid(vec![
            crate::schema::FieldError {
                field: "cropType".into(),
                message: "Must be a string".into(),
            },
        ])))),
    }
}

async fn reload(State(state): State<AppState>, body: Bytes) -> Response {
    let crop_type = match crop_type_of(&body) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    if state.learned.directory().is_none() {
        return detail(
            StatusCode::BAD_REQUEST,
            "No models directory configured for reload (empty DATA_DIR)",
        );
    }
    let report = state.learned.reload(crop_type.clone()).await;
    if !report.rejected.is_empty() {
        let reason = report
            .rejected
            .first()
            .map(|rejection| rejection.reason.clone())
            .unwrap_or_default();
        return json_value(
            StatusCode::BAD_REQUEST,
            &json!({
                "detail": format!("Model verification failed: {reason}"),
                "rejected": report.rejected,
                "disabled": report.disabled,
            }),
        );
    }
    json_value(
        StatusCode::OK,
        &json!({"status": "OK", "reloaded": report.reloaded, "rejected": report.rejected, "disabled": report.disabled}),
    )
}

fn passthrough(forwarded: Forwarded) -> Response {
    let status = StatusCode::from_u16(forwarded.status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    json_response(status, forwarded.body.to_vec())
}

async fn readings(State(state): State<AppState>, body: Bytes) -> Response {
    let Some(trainer) = &state.trainer else {
        return detail(
            StatusCode::SERVICE_UNAVAILABLE,
            "Continuous learning is unavailable: SmartPot-Trainer is not configured",
        );
    };
    let Some(forwarded) = trainer
        .forward(
            Method::POST,
            "/v1/learning/readings",
            Some(body),
            TRAINER_TIMEOUT,
        )
        .await
    else {
        return detail(
            StatusCode::SERVICE_UNAVAILABLE,
            "Continuous learning is unavailable: SmartPot-Trainer did not respond",
        );
    };
    if forwarded.status.is_success() {
        if let Ok(value) = serde_json::from_slice::<Value>(&forwarded.body) {
            let received = value.get("received").and_then(Value::as_u64).unwrap_or(0);
            let stored = value.get("stored").and_then(Value::as_u64).unwrap_or(0);
            if received > 0 && stored == 0 {
                return detail(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "SmartPot-Trainer accepted the batch but stored no readings",
                );
            }
        }
    }
    passthrough(forwarded)
}

/// Estado seguro cuando el aprendizaje no está disponible.
fn disabled_status(state: &AppState) -> Value {
    json!({
        "enabled": false,
        "persistent": false,
        "storedReadings": 0,
        "minSamples": state.settings.min_samples,
        "retrainEvery": 300,
        "cropTypes": [],
    })
}

async fn status(State(state): State<AppState>) -> Response {
    if let Some(trainer) = &state.trainer {
        if let Some(forwarded) = trainer
            .forward(Method::GET, "/v1/learning/status", None, TRAINER_TIMEOUT)
            .await
        {
            if forwarded.status.is_success() {
                if let Ok(value) = serde_json::from_slice::<Value>(&forwarded.body) {
                    trainer.absorb(&value);
                }
            }
            return passthrough(forwarded);
        }
    }
    json_value(StatusCode::OK, &disabled_status(&state))
}

async fn train(State(state): State<AppState>, body: Bytes) -> Response {
    let crop_type = match crop_type_of(&body) {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let Some(trainer) = &state.trainer else {
        return detail(
            StatusCode::SERVICE_UNAVAILABLE,
            "Training is handled by SmartPot-Trainer, which is not configured",
        );
    };
    let payload = Bytes::from(
        serde_json::to_vec(&json!({ "cropType": crop_type })).expect("JSON serializable"),
    );
    let Some(forwarded) = trainer
        .forward(
            Method::POST,
            "/v1/learning/train",
            Some(payload),
            TRAINING_TIMEOUT,
        )
        .await
    else {
        return detail(
            StatusCode::SERVICE_UNAVAILABLE,
            "SmartPot-Trainer is not responding",
        );
    };
    if forwarded.status.is_success() && state.learned.directory().is_some() {
        // El Trainer también avisa con /v1/learning/reload; recargar aquí deja la respuesta al día.
        state.learned.reload(crop_type).await;
    }
    passthrough(forwarded)
}

/// Codifica un segmento de ruta (también los puntos): el id nunca puede cambiar la ruta que se pide al Trainer.
fn segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

async fn forget(State(state): State<AppState>, Path(crop_id): Path<String>) -> Response {
    // Los ids son ObjectId en hexadecimal; con letras, dígitos, «-» y «_» el id nunca cambia la ruta del Trainer.
    let valid = !crop_id.is_empty()
        && crop_id.len() <= 64
        && crop_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !valid {
        return detail(StatusCode::UNPROCESSABLE_ENTITY, "Invalid data");
    }
    if let Some(trainer) = &state.trainer {
        let path = format!("/v1/learning/crops/{}", segment(&crop_id));
        if let Some(forwarded) = trainer
            .forward(Method::DELETE, &path, None, TRAINER_TIMEOUT)
            .await
        {
            return passthrough(forwarded);
        }
    }
    json_value(StatusCode::OK, &json!({"deleted": 0}))
}

async fn prune(State(state): State<AppState>) -> Response {
    if let Some(trainer) = &state.trainer {
        if let Some(forwarded) = trainer
            .forward(Method::POST, "/v1/learning/prune", None, TRAINER_TIMEOUT)
            .await
        {
            return passthrough(forwarded);
        }
    }
    json_value(StatusCode::OK, &json!({"removed": 0}))
}

#[cfg(test)]
mod tests {
    use super::segment;

    #[test]
    fn crop_ids_cannot_escape_the_path() {
        assert_eq!(
            segment("66f5a1000000000000000101"),
            "66f5a1000000000000000101"
        );
        assert_eq!(segment("../health"), "%2E%2E%2Fhealth");
        assert_eq!(segment("a b?c"), "a%20b%3Fc");
    }
}

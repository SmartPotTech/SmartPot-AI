//! Contrato REST: salud, autenticación, errores, catálogo, estado sin Trainer y proxy transparente hacia el Trainer.

mod common;

use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use smartpot_ia::http::router;

use common::{call, state, state_with, TOKEN};

#[tokio::test]
async fn health_is_public_and_the_rest_needs_the_token() {
    let app = router(state());
    let (status, health) = call(&app, "GET", "/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["status"], "UP");
    assert_eq!(health["service"], "smartpot-ia-rust");
    assert_eq!(health["learning"], "OFF");

    let (status, body) = call(
        &app,
        "GET",
        "/v1/crop-profiles",
        None,
        Some("another-token-that-is-not-the-api-one"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["detail"], "Invalid service token");

    let (status, body) = call(&app, "GET", "/v1/nothing", None, Some(TOKEN)).await;
    assert_eq!(
        (status, body["detail"].as_str()),
        (StatusCode::NOT_FOUND, Some("Not Found"))
    );
    let (status, _) = call(&app, "GET", "/v1/insights", None, Some(TOKEN)).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn profiles_by_species() {
    let app = router(state());
    let (status, all) = call(&app, "GET", "/v1/profiles", None, Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().unwrap().len(), 6);
    let (status, basil) = call(&app, "GET", "/v1/profiles/basil", None, Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(basil["name"], "Basil");
    assert_eq!(basil["ranges"]["soilMoisture"]["min"], 50.0);
    let (status, _) = call(&app, "GET", "/v1/profiles/CACTUS", None, Some(TOKEN)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn zero_is_a_reading_and_null_is_a_missing_sensor() {
    let app = router(state());
    let night = json!({"cropType": "TOMATO", "measures": {"brightness": 0.0, "temperature": 0.0}, "localHour": 23,
                       "actuators": ["UV_LIGHT", "FAN"]});
    let (status, body) = call(&app, "POST", "/v1/insights", Some(&night), Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    let parameters: Vec<&str> = body["diagnosis"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["parameter"].as_str().unwrap())
        .collect();
    assert_eq!(parameters, ["temperature", "brightness"]);
    assert_eq!(body["diagnosis"][1]["status"], "REST");
    assert!(body["conclusions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["rule"] == "cold_stress"));
    assert!(body["actions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["actuator"] != "UV_LIGHT"));

    let missing =
        json!({"cropType": "TOMATO", "measures": {"brightness": null, "temperature": null}});
    let (_, body) = call(&app, "POST", "/v1/insights", Some(&missing), Some(TOKEN)).await;
    assert!(body["diagnosis"].as_array().unwrap().is_empty());
    assert_eq!(body["health"]["level"], "UNKNOWN");
}

#[tokio::test]
async fn measured_at_derives_the_local_hour_and_explicit_hour_wins() {
    let app = router(state());
    // 04:00 UTC = 23:00 in America/Bogota: la luz baja es descanso nocturno.
    let derived = json!({
        "cropType": "LETTUCE",
        "measures": {"brightness": 0.0, "measuredAt": "2026-11-02T04:00:00Z"},
        "actuators": ["UV_LIGHT"],
    });
    let (status, body) = call(&app, "POST", "/v1/insights", Some(&derived), Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    let brightness = body["diagnosis"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["parameter"] == "brightness")
        .unwrap();
    assert_eq!(brightness["status"], "REST");
    assert!(body["actions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|action| action["actuator"] != "UV_LIGHT"));

    // La hora explícita manda sobre measuredAt.
    let explicit = json!({
        "cropType": "LETTUCE",
        "measures": {"brightness": 0.0, "measuredAt": "2026-11-02T04:00:00Z"},
        "localHour": 12,
        "actuators": ["UV_LIGHT"],
    });
    let (_, body) = call(&app, "POST", "/v1/insights", Some(&explicit), Some(TOKEN)).await;
    let brightness = body["diagnosis"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["parameter"] == "brightness")
        .unwrap();
    assert_eq!(brightness["status"], "LOW");
    assert!(body["actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action["actuator"] == "UV_LIGHT" && action["action"] == "ACTIVATE"));
}

#[tokio::test]
async fn invalid_json_is_unprocessable() {
    let app = router(state());
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/fleet")
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(axum::body::Body::from("{not json"))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn learning_without_trainer_answers_a_safe_disabled_state() {
    let app = router(state());
    let (status, body) = call(&app, "GET", "/v1/learning/status", None, Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], false);
    assert_eq!(body["cropTypes"], json!([]));
    let batch = json!({"readings": [{"cropId": "a", "cropType": "LETTUCE", "measuredAt": "2026-11-02T10:00:00Z", "measures": {}}]});
    let (status, body) = call(
        &app,
        "POST",
        "/v1/learning/readings",
        Some(&batch),
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body["detail"].as_str().unwrap().contains("Trainer"));
    let (status, body) = call(
        &app,
        "DELETE",
        "/v1/learning/crops/66f5a1000000000000000101",
        None,
        Some(TOKEN),
    )
    .await;
    assert_eq!((status, body), (StatusCode::OK, json!({"deleted": 0})));
    let (status, _) = call(
        &app,
        "POST",
        "/v1/learning/train",
        Some(&json!({})),
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let (status, _) = call(
        &app,
        "POST",
        "/v1/learning/reload",
        Some(&json!({})),
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Lo que recibió el Trainer simulado: método, ruta, cuerpo y cabecera de autorización.
type Seen = Arc<Mutex<Vec<(String, String, String, String)>>>;

async fn mock_trainer() -> (String, Seen) {
    let seen: Seen = Arc::default();
    fn record(seen: &Seen, method: &str, path: String, body: &[u8], headers: &HeaderMap) {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        seen.lock().unwrap().push((
            method.into(),
            path,
            String::from_utf8_lossy(body).into_owned(),
            auth,
        ));
    }
    let app = Router::new()
        .route(
            "/v1/learning/readings",
            post(|State(seen): State<Seen>, headers: HeaderMap, body: Bytes| async move {
                record(&seen, "POST", "/v1/learning/readings".into(), &body, &headers);
                Json(json!({"received": 1, "stored": 1, "total": 42}))
            }),
        )
        .route(
            "/v1/learning/status",
            get(|State(seen): State<Seen>, headers: HeaderMap| async move {
                record(&seen, "GET", "/v1/learning/status".into(), b"", &headers);
                Json(json!({"enabled": true, "persistent": true, "storedReadings": 42, "minSamples": 150,
                            "retrainEvery": 300, "cropTypes": [{"cropType": "LETTUCE", "readings": 42}]}))
            }),
        )
        .route(
            "/v1/learning/crops/{id}",
            delete(|State(seen): State<Seen>, uri: Uri, headers: HeaderMap| async move {
                record(&seen, "DELETE", uri.path().to_string(), b"", &headers);
                (StatusCode::OK, Json(json!({"deleted": 7})))
            }),
        )
        .route(
            "/v1/learning/train",
            post(|State(seen): State<Seen>, headers: HeaderMap, body: Bytes| async move {
                record(&seen, "POST", "/v1/learning/train".into(), &body, &headers);
                (StatusCode::NOT_FOUND, Json(json!({"detail": "No readings for BASIL"})))
            }),
        )
        .with_state(Arc::clone(&seen));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), seen)
}

#[tokio::test]
async fn learning_routes_are_forwarded_to_the_trainer_byte_for_byte() {
    let (url, seen) = mock_trainer().await;
    let app = router(state_with(&[
        ("SMARTPOT_TRAINER_URL", &url),
        ("SMARTPOT_TRAINER_TOKEN", "trainer-own-token-123"),
    ]));

    let batch = json!({"readings": [{"cropId": "a", "cropType": "LETTUCE", "measuredAt": "2026-11-02T10:00:00.123456-05:00",
                                     "measures": {"temperature": 0.0}}]});
    let (status, body) = call(
        &app,
        "POST",
        "/v1/learning/readings",
        Some(&batch),
        Some(TOKEN),
    )
    .await;
    assert_eq!((status, body["total"].clone()), (StatusCode::OK, json!(42)));
    let (status, body) = call(&app, "GET", "/v1/learning/status", None, Some(TOKEN)).await;
    assert_eq!(
        (status, body["minSamples"].clone()),
        (StatusCode::OK, json!(150))
    );
    let (status, _) = call(
        &app,
        "DELETE",
        "/v1/learning/crops/..%2Fhealth",
        None,
        Some(TOKEN),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "an id that changes the path never reaches the Trainer"
    );
    let (status, body) = call(
        &app,
        "DELETE",
        "/v1/learning/crops/66f5a1000000000000000101",
        None,
        Some(TOKEN),
    )
    .await;
    assert_eq!((status, body), (StatusCode::OK, json!({"deleted": 7})));
    let (status, body) = call(
        &app,
        "POST",
        "/v1/learning/train",
        Some(&json!({"cropType": "basil"})),
        Some(TOKEN),
    )
    .await;
    assert_eq!(
        (status, body["detail"].as_str()),
        (StatusCode::NOT_FOUND, Some("No readings for BASIL"))
    );

    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen[0].2,
        serde_json::to_string(&batch).unwrap(),
        "the body travels without reinterpreting dates"
    );
    assert!(seen
        .iter()
        .all(|(_, _, _, auth)| auth == "Bearer trainer-own-token-123"));
    assert_eq!(seen.len(), 4);
    assert_eq!(seen[2].1, "/v1/learning/crops/66f5a1000000000000000101");
    assert_eq!(seen[3].2, r#"{"cropType":"BASIL"}"#);

    // Con el estado del Trainer, la evaluación informa sus conteos.
    let insight = json!({"cropType": "LETTUCE", "measures": {"temperature": 18.0}});
    let (_, body) = call(&app, "POST", "/v1/insights", Some(&insight), Some(TOKEN)).await;
    let message: &Value = &body["learning"]["message"];
    assert_eq!(body["learning"]["readings"], 42);
    assert!(
        message.as_str().unwrap().contains("starts at 150"),
        "{message}"
    );
}

#[tokio::test]
async fn an_unreachable_trainer_does_not_break_the_api() {
    let app = router(state_with(&[(
        "SMARTPOT_TRAINER_URL",
        "http://127.0.0.1:9",
    )]));
    let (status, body) = call(&app, "GET", "/v1/learning/status", None, Some(TOKEN)).await;
    assert_eq!(
        (status, body["enabled"].clone()),
        (StatusCode::OK, json!(false))
    );
    let (status, _) = call(
        &app,
        "POST",
        "/v1/learning/train",
        Some(&json!({})),
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let insight = json!({"cropType": "LETTUCE", "measures": {"temperature": 18.0}});
    let (status, _) = call(&app, "POST", "/v1/insights", Some(&insight), Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
}

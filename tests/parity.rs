//! Paridad con el servicio en Python: las mismas peticiones deben dar las mismas respuestas.
//!
//! Los casos los genera `tools/generate_parity_fixtures.py` contra SmartPot-AI. En las peticiones inválidas se exige el
//! mismo código, el mismo `detail` y los mismos campos con error (el texto de cada error está en español aquí).

mod common;

use axum::http::StatusCode;
use serde_json::Value;
use smartpot_ia::http::router;

use common::{call, differences, fixture, state, TOKEN};

fn error_fields(body: &Value) -> Vec<String> {
    let mut fields: Vec<String> = body["errors"]
        .as_array()
        .map(|errors| {
            errors
                .iter()
                .map(|e| e["field"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default();
    fields.sort();
    fields.dedup();
    fields
}

async fn compare(path: &str, cases: &Value) -> Vec<String> {
    let app = router(state());
    let mut failures = Vec::new();
    for (i, case) in cases.as_array().unwrap().iter().enumerate() {
        let (status, body) = call(&app, "POST", path, Some(&case["request"]), Some(TOKEN)).await;
        let expected_status = case["status"].as_u64().unwrap() as u16;
        if status.as_u16() != expected_status {
            failures.push(format!(
                "caso {i}: código {status} ≠ {expected_status}: {body}"
            ));
            continue;
        }
        if status == StatusCode::UNPROCESSABLE_ENTITY {
            let expected = &case["response"];
            if body["detail"] != expected["detail"] || error_fields(&body) != error_fields(expected)
            {
                failures.push(format!("caso {i}: errores {body} ≠ {expected}"));
            }
            continue;
        }
        let mut found = Vec::new();
        differences("", &case["response"], &body, &mut found);
        if !found.is_empty() {
            failures.push(format!("caso {i}: {}", found.join("; ")));
        }
    }
    failures
}

#[tokio::test]
async fn insights_match_the_python_service() {
    let cases = fixture("parity_insights.json");
    let failures = compare("/v1/insights", &cases).await;
    assert!(
        failures.is_empty(),
        "{} de {} casos distintos:\n{}",
        failures.len(),
        cases.as_array().unwrap().len(),
        failures.join("\n")
    );
}

#[tokio::test]
async fn fleet_matches_the_python_service() {
    // Incluye los grupos: K-Means repite la inicialización y las iteraciones de scikit-learn con la misma semilla.
    let cases = fixture("parity_fleet.json");
    let failures = compare("/v1/fleet", &cases).await;
    assert!(
        failures.is_empty(),
        "{} casos distintos:
{}",
        failures.len(),
        failures.join(
            "
"
        )
    );
}

#[tokio::test]
async fn catalog_models_and_auth_match() {
    let extra = fixture("parity_extra.json");
    let app = router(state());
    let (status, profiles) = call(&app, "GET", "/v1/crop-profiles", None, Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    let mut found = Vec::new();
    differences(
        "crop_profiles",
        &extra["crop_profiles"],
        &profiles,
        &mut found,
    );
    let (_, models) = call(&app, "GET", "/v1/models", None, Some(TOKEN)).await;
    differences("models", &extra["models"], &models, &mut found);
    assert!(found.is_empty(), "{}", found.join("\n"));
    let (status, body) = call(
        &app,
        "POST",
        "/v1/insights",
        Some(&serde_json::json!({})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, extra["unauthorized"]);
}

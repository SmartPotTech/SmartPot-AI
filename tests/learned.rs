//! Modelos aprendidos por SmartPot-Trainer: verificación, recarga en caliente y paridad con el servicio en Python.
//!
//! Los archivos los genera `tools/generate_learned_fixtures.py`: el Trainer entrena lechuga con datos sintéticos y
//! firma el paquete con una clave de prueba; SmartPot-AI responde las mismas evaluaciones con esos modelos.

mod common;

use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use serde_json::Value;
use smartpot_ia::http::router;
use smartpot_ia::state::AppState;

use common::{call, differences, state_with, TOKEN};

fn learned_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("learned")
}

fn trusted_key() -> String {
    std::fs::read_to_string(learned_dir().join("trusted_key.txt"))
        .unwrap()
        .trim()
        .to_string()
}

fn state_for(dir: &Path, key: Option<&str>) -> AppState {
    let dir = dir.to_string_lossy().to_string();
    let mut pairs = vec![("MODELS_DIR", dir.as_str())];
    if let Some(key) = key {
        pairs.push(("TRUSTED_MODEL_PUBLIC_KEY_HEX", key));
    }
    state_with(&pairs)
}

fn copy_fixtures(files: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for file in files {
        std::fs::copy(learned_dir().join(file), dir.path().join(file)).unwrap();
    }
    dir
}

#[tokio::test]
async fn signed_models_reload_and_match_the_python_service() {
    let key = trusted_key();
    let state = state_for(&learned_dir(), Some(&key));
    let report = state.learned.reload(None).await;
    assert_eq!(report.reloaded, ["LETTUCE"], "{report:?}");
    assert!(
        report.rejected.is_empty() && report.disabled.is_empty(),
        "{report:?}"
    );

    let app = router(state);
    let cases: Value = serde_json::from_str(
        &std::fs::read_to_string(learned_dir().join("expected_insights.json")).unwrap(),
    )
    .unwrap();
    let mut failures = Vec::new();
    let mut learned = 0;
    for (i, case) in cases.as_array().unwrap().iter().enumerate() {
        let (status, body) = call(
            &app,
            "POST",
            "/v1/insights",
            Some(&case["request"]),
            Some(TOKEN),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        if !body["learning"]["predictions"]
            .as_array()
            .unwrap()
            .is_empty()
        {
            learned += 1;
        }
        let mut found = Vec::new();
        differences("", &case["response"], &body, &mut found);
        if !found.is_empty() {
            failures.push(format!("case {i}: {}", found.join("; ")));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differing cases:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(learned >= 20, "too few learned predictions: {learned}");
}

#[tokio::test]
async fn the_reload_endpoint_reports_what_it_loaded() {
    let key = trusted_key();
    let app = router(state_for(&learned_dir(), Some(&key)));
    let body = serde_json::json!({"cropType": "lettuce"});
    let (status, report) = call(
        &app,
        "POST",
        "/v1/learning/reload",
        Some(&body),
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["reloaded"], serde_json::json!(["LETTUCE"]));
    let (_, health) = call(&app, "GET", "/health", None, None).await;
    assert_eq!(health["learnedCropTypes"], serde_json::json!(["LETTUCE"]));
    assert_eq!(health["signedModels"], true);
}

#[tokio::test]
async fn tampered_or_unsigned_models_are_rejected_with_a_trusted_key() {
    let key = trusted_key();

    let tampered = copy_fixtures(&["LETTUCE_inference.json", "LETTUCE_manifest.json"]);
    let path = tampered.path().join("LETTUCE_inference.json");
    let text = std::fs::read_to_string(&path).unwrap().replacen(
        "\"min_samples\":100",
        "\"min_samples\":101",
        1,
    );
    std::fs::write(&path, text).unwrap();
    let report = state_for(tampered.path(), Some(&key))
        .learned
        .reload(Some("LETTUCE".into()))
        .await;
    assert!(report.reloaded.is_empty());
    assert!(report.rejected[0].reason.contains("BLAKE2b"), "{report:?}");

    let unsigned = copy_fixtures(&["LETTUCE_inference.json"]);
    let report = state_for(unsigned.path(), Some(&key))
        .learned
        .reload(None)
        .await;
    assert!(report.rejected[0].reason.contains("manifest"), "{report:?}");

    let other_key = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    let report = state_for(&learned_dir(), Some(other_key))
        .learned
        .reload(None)
        .await;
    assert!(
        report.rejected[0].reason.contains("signature"),
        "{report:?}"
    );

    let app = router(state_for(tampered.path(), Some(&key)));
    let body = serde_json::json!({"cropType": "LETTUCE"});
    let (status, error) = call(
        &app,
        "POST",
        "/v1/learning/reload",
        Some(&body),
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["detail"]
        .as_str()
        .unwrap()
        .starts_with("Model verification failed"));
}

#[tokio::test]
async fn without_a_trusted_key_only_integrity_is_checked() {
    let unsigned = copy_fixtures(&["LETTUCE_inference.json"]);
    let report = state_for(unsigned.path(), None).learned.reload(None).await;
    assert_eq!(report.reloaded, ["LETTUCE"]);
}

#[tokio::test]
async fn the_native_trainer_export_passes_all_sixteen_controls() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("learned_native");
    let key = std::fs::read_to_string(dir.join("trusted_key.txt"))
        .unwrap()
        .trim()
        .to_string();
    let bundle: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("LETTUCE_inference.json")).unwrap())
            .unwrap();
    assert_eq!(bundle["checks"].as_array().unwrap().len(), 16);

    let path = dir.to_string_lossy().to_string();
    let state = state_with(&[
        ("MODELS_DIR", path.as_str()),
        ("TRUSTED_MODEL_PUBLIC_KEY_HEX", key.as_str()),
    ]);
    let report = state.learned.reload(Some("LETTUCE".into())).await;
    assert_eq!(report.reloaded, ["LETTUCE"], "{report:?}");
    assert!(report.rejected.is_empty(), "{report:?}");
    assert!(report.disabled.is_empty(), "{report:?}");
}

#[tokio::test]
async fn a_model_that_disagrees_with_scikit_learn_is_disabled() {
    let dir = copy_fixtures(&["LETTUCE_inference.json"]);
    let path = dir.path().join("LETTUCE_inference.json");
    let mut bundle: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let first = bundle["checks"][0]["needs_water"].as_f64().unwrap();
    bundle["checks"][0]["needs_water"] = Value::from(first + 0.01);
    std::fs::write(&path, serde_json::to_vec(&bundle).unwrap()).unwrap();

    let state = state_for(dir.path(), None);
    let report = state.learned.reload(None).await;
    assert_eq!(report.reloaded, ["LETTUCE"]);
    assert!(
        report.disabled.iter().any(|d| d.contains("needs_water")),
        "{report:?}"
    );
    let models = state.learned.get("LETTUCE").unwrap();
    assert!(!models.card("needs_water").unwrap().usable());
    assert!(models.card("overheat").unwrap().usable());
}

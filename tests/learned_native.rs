//! Paquete nativo generado por SmartPot-Trainer (`tools/generate_learned_fixtures.py`).
//!
//! Comprueba la cadena completa: JSON `smartpot.learned-models/1`, manifiesto Ed25519/BLAKE2b,
//! los 16 casos de control y la recarga en Rust sin ningún submodelo deshabilitado.

mod common;

use std::path::PathBuf;

use serde_json::Value;

use axum::http::StatusCode;
use smartpot_ia::http::router;

use common::{call, state_with, TOKEN};

fn learned_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("learned_native")
}

fn trusted_key() -> String {
    std::fs::read_to_string(learned_dir().join("trusted_key.txt"))
        .unwrap()
        .trim()
        .to_string()
}

#[tokio::test]
async fn native_trainer_export_loads_without_disabled_models() {
    let key = trusted_key();
    let dir = learned_dir().to_string_lossy().to_string();
    let state = state_with(&[
        ("MODELS_DIR", dir.as_str()),
        ("TRUSTED_MODEL_PUBLIC_KEY_HEX", key.as_str()),
        ("SMARTPOT_AI_TOKEN", TOKEN),
    ]);

    let bundle: Value = serde_json::from_str(
        &std::fs::read_to_string(learned_dir().join("LETTUCE_inference.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(bundle["schema"], "smartpot.learned-models/1");
    assert_eq!(bundle["features"], 10);
    assert_eq!(bundle["checks"].as_array().unwrap().len(), 16);
    assert_eq!(bundle["cards"].as_array().unwrap().len(), 3);

    let report = state.learned.reload(Some("LETTUCE".into())).await;
    assert_eq!(report.reloaded, ["LETTUCE"], "{report:?}");
    assert!(report.rejected.is_empty(), "{report:?}");
    assert!(report.disabled.is_empty(), "{report:?}");

    let loaded = state.learned.get("LETTUCE").expect("loaded bundle");
    assert_eq!(loaded.trained_cards(), 3);
    assert!(loaded.states.is_some());
    assert!(loaded.anomaly.is_some());
}

#[tokio::test]
async fn an_empty_control_list_is_not_loadable() {
    let dir = tempfile::tempdir().unwrap();
    let mut bundle: Value = serde_json::from_str(
        &std::fs::read_to_string(learned_dir().join("LETTUCE_inference.json")).unwrap(),
    )
    .unwrap();
    bundle["checks"] = Value::Array(Vec::new());
    std::fs::write(
        dir.path().join("LETTUCE_inference.json"),
        serde_json::to_vec(&bundle).unwrap(),
    )
    .unwrap();

    let path = dir.path().to_string_lossy().to_string();
    let state = state_with(&[("MODELS_DIR", path.as_str()), ("SMARTPOT_AI_TOKEN", TOKEN)]);
    let report = state.learned.reload(Some("LETTUCE".into())).await;
    assert!(report.reloaded.is_empty(), "{report:?}");
    assert!(report.rejected[0].reason.contains("16"), "{report:?}");
}

#[tokio::test]
async fn an_oversized_manifest_is_rejected_before_reading_it() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        learned_dir().join("LETTUCE_inference.json"),
        dir.path().join("LETTUCE_inference.json"),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("LETTUCE_manifest.json"),
        vec![b' '; 1024 * 1024 + 1],
    )
    .unwrap();

    let path = dir.path().to_string_lossy().to_string();
    let state = state_with(&[("MODELS_DIR", path.as_str())]);
    let report = state.learned.reload(Some("LETTUCE".into())).await;
    assert!(report.reloaded.is_empty(), "{report:?}");
    assert!(
        report.rejected[0].reason.contains("manifest file exceeds"),
        "{report:?}"
    );
}

#[tokio::test]
async fn a_global_reload_with_a_rejected_species_returns_bad_request() {
    let dir = tempfile::tempdir().unwrap();
    for file in ["LETTUCE_inference.json", "LETTUCE_manifest.json"] {
        std::fs::copy(learned_dir().join(file), dir.path().join(file)).unwrap();
    }
    // El archivo de BASIL contiene una especie distinta: se rechaza aunque el manifiesto sea válido.
    std::fs::copy(
        learned_dir().join("LETTUCE_inference.json"),
        dir.path().join("BASIL_inference.json"),
    )
    .unwrap();
    std::fs::copy(
        learned_dir().join("LETTUCE_manifest.json"),
        dir.path().join("BASIL_manifest.json"),
    )
    .unwrap();

    let path = dir.path().to_string_lossy().to_string();
    let app = router(state_with(&[("MODELS_DIR", path.as_str())]));
    let (status, body) = call(
        &app,
        "POST",
        "/v1/learning/reload",
        Some(&serde_json::json!({})),
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["detail"]
        .as_str()
        .unwrap()
        .starts_with("Model verification failed"));
    assert!(body["rejected"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));
}

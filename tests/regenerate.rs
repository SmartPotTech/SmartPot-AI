//! Regenera las respuestas de los fixtures de paridad con el motor actual.
//!
//! Se ejecuta solo a mano, después de un cambio deliberado del contrato:
//! `SMARTPOT_REGENERATE_FIXTURES=1 cargo test --test regenerate -- --ignored`.

mod common;

use std::path::PathBuf;

use serde_json::{json, Value};

use common::{state, TOKEN};
use smartpot_ia::services;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn regenerate(name: &str, fleet: bool) {
    let path = fixture_path(name);
    let mut cases: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let state = state();
    for case in cases.as_array_mut().unwrap() {
        if case["status"].as_u64() != Some(200) {
            continue;
        }
        let request = serde_json::to_vec(&case["request"]).unwrap();
        let response = if fleet {
            services::fleet(&state, &request)
        } else {
            services::insight(&state, &request)
        };
        if let Ok(bytes) = response {
            case["response"] = serde_json::from_slice(&bytes).unwrap();
        }
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    println!("regenerated {}", path.display());
}

#[test]
#[ignore]
fn regenerate_parity_fixtures() {
    if std::env::var("SMARTPOT_REGENERATE_FIXTURES")
        .ok()
        .as_deref()
        != Some("1")
    {
        return;
    }
    // El token se lee de la configuración de prueba; se deja constancia para evitar dudas.
    assert_eq!(TOKEN, "parity-token-for-local-tests-only");
    regenerate("parity_insights.json", false);
    regenerate("parity_fleet.json", true);
    let _ = json!({});
}

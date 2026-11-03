//! Los modelos base pre-empacados dan lo mismo que scikit-learn (casos de `tools/export_base_models.py`).

mod common;

use smartpot_ia::domain::Parameter;
use smartpot_ia::engine::models::BaseModels;

use common::fixture;

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0)
}

#[test]
fn base_models_match_scikit_learn() {
    let golden = fixture("base_models_golden.json");
    let models = BaseModels::embedded().unwrap();
    let positions = golden["positions"].as_array().unwrap();
    for (i, pair) in positions.iter().enumerate() {
        let (a, b) = (pair[0].as_f64().unwrap(), pair[1].as_f64().unwrap());
        let mut values = [None; 6];
        values[Parameter::Temperature.index()] = Some(a);
        values[Parameter::Humidity.index()] = Some(b);
        values[Parameter::Ph.index()] = Some(a);
        values[Parameter::Tds.index()] = Some(b);
        let ventilation = models.ventilation_probability(&values).unwrap();
        let ph = models.ph_correction_probability(&values).unwrap();
        assert!(
            close(ventilation, golden["ventilation"][i].as_f64().unwrap()),
            "ventilación {i}"
        );
        assert!(
            close(ph, golden["ph_correction"][i].as_f64().unwrap()),
            "pH {i}"
        );
    }
    for (i, vector) in golden["vectors"].as_array().unwrap().iter().enumerate() {
        let x: Vec<f64> = vector
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        assert!(
            close(
                models.anomaly_decision(&x),
                golden["anomaly_decision"][i].as_f64().unwrap()
            ),
            "atípico {i}"
        );
    }
    for (name, value) in &models.metrics {
        let expected = golden["metrics"][name].as_f64().unwrap();
        assert!(
            (smartpot_ia::format::py_round(*value, 4) - expected).abs() < 1e-12,
            "{name}"
        );
    }
}

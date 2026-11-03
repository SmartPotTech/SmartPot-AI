"""Exporta los modelos base de SmartPot-AI (semilla 42) al formato JSON que carga SmartPot-IA-Rust.

Entrena exactamente los mismos modelos que el servicio en Python (misma semilla, mismos datos sintéticos y la misma
versión de scikit-learn) y guarda sus parámetros: escalado, coeficientes, capas de la red y árboles del Isolation
Forest. Antes de escribir comprueba que la evaluación con esos parámetros coincide con scikit-learn, y deja casos de
referencia para las pruebas de Rust.

Uso, desde la raíz de SmartPot-IA-Rust y con el entorno de SmartPot-AI:

    uv run --project ../SmartPot-AI python tools/export_base_models.py ../SmartPot-AI
"""

import json
import sys
from pathlib import Path

import numpy as np
import sklearn
from sklearn.ensemble._iforest import _average_path_length

ROOT = Path(__file__).resolve().parent.parent
SEED = 42


def scaler(step) -> dict:
    return {"mean": step.mean_.tolist(), "scale": step.scale_.tolist()}


def logistic(pipeline) -> dict:
    scale, model = pipeline.steps[0][1], pipeline.steps[-1][1]
    return {
        "kind": "logistic",
        "scaler": scaler(scale),
        "coef": model.coef_[0].tolist(),
        "intercept": float(model.intercept_[0]),
    }


def mlp(pipeline) -> dict:
    scale, model = pipeline.steps[0][1], pipeline.steps[-1][1]
    return {
        "kind": "mlp",
        "scaler": scaler(scale),
        "hidden_activation": model.activation,
        "output_activation": model.out_activation_,
        "layers": [
            {"weights": weights.tolist(), "bias": bias.tolist()}
            for weights, bias in zip(model.coefs_, model.intercepts_, strict=True)
        ],
    }


def isolation_forest(forest) -> dict:
    trees = []
    for tree, depths, lengths in zip(
        forest.estimators_, forest._decision_path_lengths, forest._average_path_length_per_tree, strict=True
    ):
        t = tree.tree_
        # Mismo cálculo que scikit-learn por hoja: profundidad + largo promedio del resto - 1.
        value = depths + lengths - 1.0
        trees.append({
            "left": t.children_left.tolist(),
            "right": t.children_right.tolist(),
            "feature": t.feature.tolist(),
            "threshold": t.threshold.tolist(),
            "value": value.tolist(),
        })
    denominator = len(forest.estimators_) * float(_average_path_length([forest._max_samples])[0])
    return {
        "kind": "isolation_forest",
        "denominator": denominator,
        "offset": float(forest.offset_),
        "trees": trees,
    }


def forest_decision(spec: dict, x: np.ndarray) -> np.ndarray:
    """Evaluación de referencia con los parámetros exportados (la misma que hace Rust)."""
    x32 = x.astype(np.float32).astype(np.float64)
    out = np.empty(x.shape[0])
    for row in range(x.shape[0]):
        depth = 0.0
        for tree in spec["trees"]:
            node = 0
            while tree["left"][node] != -1:
                feature = tree["feature"][node]
                node = tree["left"][node] if x32[row, feature] <= tree["threshold"][node] else tree["right"][node]
            depth += tree["value"][node]
        out[row] = -(2.0 ** (-(depth / spec["denominator"]))) - spec["offset"]
    return out


def main() -> None:
    ai = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT.parent / "SmartPot-AI").resolve()
    sys.path.insert(0, str(ai))
    from app.engine.models import ModelRegistry  # noqa: PLC0415 - depende de la ruta recibida

    registry = ModelRegistry(seed=SEED)
    payload = {
        "schema": "smartpot.base-models/1",
        "seed": SEED,
        "sklearn": sklearn.__version__,
        "metrics": {name: float(value) for name, value in registry.metrics.items()},
        "ventilation": logistic(registry.ventilation),
        "ph_correction": mlp(registry.ph_correction),
        "anomaly": isolation_forest(registry.anomaly),
    }

    rng = np.random.default_rng(2026)
    positions = rng.uniform(-2.0, 3.0, size=(400, 2))
    vectors = np.vstack([rng.uniform(-1.5, 2.5, size=(300, 6)), rng.uniform(3.0, 7.5, size=(100, 6))])

    ventilation = registry.ventilation.predict_proba(registry._ventilation_features(positions))[:, 1]
    ph = registry.ph_correction.predict_proba(positions)[:, 1]
    anomaly = registry.anomaly.decision_function(vectors)

    exported = forest_decision(payload["anomaly"], vectors)
    gap = float(np.max(np.abs(exported - anomaly)))
    if gap > 1e-12:
        raise SystemExit(f"El bosque exportado no coincide con scikit-learn (diferencia {gap})")

    golden = {
        "positions": positions.tolist(),
        "ventilation": ventilation.tolist(),
        "ph_correction": ph.tolist(),
        "vectors": vectors.tolist(),
        "anomaly_decision": anomaly.tolist(),
        "metrics": {name: round(value, 4) for name, value in registry.metrics.items()},
    }

    models = ROOT / "models" / "base_models.json"
    models.parent.mkdir(parents=True, exist_ok=True)
    models.write_text(json.dumps(payload, separators=(",", ":")), encoding="utf-8")
    fixtures = ROOT / "tests" / "fixtures" / "base_models_golden.json"
    fixtures.parent.mkdir(parents=True, exist_ok=True)
    fixtures.write_text(json.dumps(golden, separators=(",", ":")), encoding="utf-8")
    print(f"Modelos base: {models} ({models.stat().st_size} bytes); casos de referencia: {fixtures}")


if __name__ == "__main__":
    main()

"""Generate a signed learned-model bundle for the Rust parity tests.

Run it with the SmartPot-Trainer environment:

    uv run --project ../SmartPot-Trainer python tools/generate_learned_fixtures.py ../SmartPot-Trainer

The script trains lettuce on synthetic readings, exports `LETTUCE_inference.json` with exactly 16
control cases and signs it with a fixed test key. SmartPot-AI loads the package with
`TRUSTED_MODEL_PUBLIC_KEY_HEX` and verifies that no model is disabled.
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "tests" / "fixtures" / "learned_native"
# Clave Ed25519 solo para pruebas: 32 bytes fijos.
TEST_KEY = bytes(range(32))


def train(trainer: Path) -> None:
    sys.path.insert(0, str(trainer))
    from cryptography.hazmat.primitives.asymmetric import ed25519  # noqa: PLC0415

    from app.learning.exporter import export_crop_models  # noqa: PLC0415
    from app.learning.service import fit_type  # noqa: PLC0415
    from app.learning.store import ReadingStore  # noqa: PLC0415
    from tests.learning_data import lettuce_series  # noqa: PLC0415

    readings = lettuce_series("crop-a", days=3.0) + lettuce_series("crop-b", days=3.0, seed=2)
    store = ReadingStore(None)
    store.add(readings)
    state = fit_type("LETTUCE", store.load("LETTUCE", 12_000), 100, None)
    store.close()

    TARGET.mkdir(parents=True, exist_ok=True)
    for old in TARGET.glob("*"):
        old.unlink()
    key = ed25519.Ed25519PrivateKey.from_private_bytes(TEST_KEY)
    export_crop_models(
        TARGET,
        "LETTUCE",
        state,
        key,
        min_samples=100,
        onnx=False,
        trusted_public_key_hex=key.public_key().public_bytes_raw().hex(),
    )
    (TARGET / "trusted_key.txt").write_text(key.public_key().public_bytes_raw().hex(), encoding="utf-8")
    bundle = json.loads((TARGET / "LETTUCE_inference.json").read_text(encoding="utf-8"))
    print(
        f"Models in {TARGET}; readings={state.readings_at_training}; "
        f"checks={len(bundle['checks'])}; cards={len(bundle['cards'])}"
    )


if __name__ == "__main__":
    project = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT.parent / "SmartPot-Trainer").resolve()
    train(project)

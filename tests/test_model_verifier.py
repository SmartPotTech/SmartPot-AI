import json
from pathlib import Path

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ed25519

from app.engine.model_verifier import compute_blake2b, verify_model_manifest


def test_model_verifier_valid_and_tampered(tmp_path: Path):
    priv_key = ed25519.Ed25519PrivateKey.generate()
    pub_hex = priv_key.public_key().public_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PublicFormat.Raw,
    ).hex()

    artifact = tmp_path / "model.onnx"
    artifact.write_bytes(b"onnx-model-binary-data")
    art_hash = compute_blake2b(artifact)

    manifest_data = {
        "schema_version": "1.0",
        "crop_type": "LETTUCE",
        "public_key_hex": pub_hex,
        "artifacts": {
            "model.onnx": {
                "blake2b": art_hash,
                "size_bytes": artifact.stat().st_size,
            }
        },
    }

    canonical_bytes = json.dumps(manifest_data, sort_keys=True, separators=(",", ":")).encode("utf-8")
    sig = priv_key.sign(canonical_bytes)

    signed_manifest = dict(manifest_data)
    signed_manifest["signature_hex"] = sig.hex()

    manifest_file = tmp_path / "manifest.json"
    manifest_file.write_text(json.dumps(signed_manifest, indent=2), encoding="utf-8")

    # Verificación exitosa
    valid, reason, _ = verify_model_manifest(manifest_file, trusted_public_key_hex=pub_hex)
    assert valid is True
    assert reason == "OK"

    # Manipulación de artefacto
    artifact.write_bytes(b"poisoned-weights")
    valid_tampered, reason_tampered, _ = verify_model_manifest(manifest_file, trusted_public_key_hex=pub_hex)
    assert valid_tampered is False
    assert "Hash mismatch" in reason_tampered

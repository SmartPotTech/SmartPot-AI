"""Verificador de integridad criptográfica y anti-tampering para modelos y adaptadores aprendidos.

Asegura que los modelos exportados por SmartPot-Trainer contengan firmas digitales válidas
(Ed25519) y resúmenes de integridad BLAKE2b antes de ser cargados en memoria.
"""

import hashlib
import json
import logging
from pathlib import Path
from typing import Any

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric import ed25519

log = logging.getLogger(__name__)


def compute_blake2b(path: Path) -> str:
    """Calcula el hash BLAKE2b (digest de 256 bits) del archivo."""
    hasher = hashlib.blake2b(digest_size=32)
    with path.open("rb") as f:
        while chunk := f.read(65536):
            hasher.update(chunk)
    return hasher.hexdigest()


def verify_model_manifest(
    manifest_path: Path,
    trusted_public_key_hex: str | None = None,
) -> tuple[bool, str, dict[str, Any]]:
    """Verifica la firma Ed25519 y la integridad BLAKE2b de los artefactos declarados en el manifiesto.

    Retorna:
        (es_valido, motivo_o_error, manifest_dict)
    """
    if not manifest_path.exists():
        return False, f"El manifiesto {manifest_path} no existe", {}

    try:
        content = json.loads(manifest_path.read_text(encoding="utf-8"))
        sig_hex = content.get("signature_hex")
        pub_hex = content.get("public_key_hex")
        artifacts = content.get("artifacts", {})

        if not sig_hex or not pub_hex:
            return False, "Manifiesto carece de firma o clave pública", {}

        if trusted_public_key_hex and pub_hex != trusted_public_key_hex:
            return False, f"Clave pública no coincide con la de confianza ({pub_hex} != {trusted_public_key_hex})", {}

        # Reconstruir datos canónicos para validación de firma
        data_to_verify = {k: v for k, v in content.items() if k != "signature_hex"}
        canonical_bytes = json.dumps(data_to_verify, sort_keys=True, separators=(",", ":")).encode("utf-8")

        pub_key = ed25519.Ed25519PublicKey.from_public_bytes(bytes.fromhex(pub_hex))
        pub_key.verify(bytes.fromhex(sig_hex), canonical_bytes)

        # Validar la integridad BLAKE2b de cada artefacto
        base_dir = manifest_path.parent
        for filename, meta in artifacts.items():
            artifact_file = base_dir / filename
            if not artifact_file.exists():
                return False, f"Artefacto faltante: {filename}", content

            expected_hash = meta.get("blake2b")
            actual_hash = compute_blake2b(artifact_file)
            if actual_hash != expected_hash:
                return False, f"Hash mismatch en {filename}: esperado {expected_hash} != {actual_hash}", content

        log.info("Verificación criptográfica exitosa para manifiesto %s", manifest_path.name)
        return True, "OK", content

    except InvalidSignature:
        return False, "Firma digital Ed25519 inválida o manifiesto manipulado", {}
    except Exception as error:  # noqa: BLE001
        return False, f"Error al procesar manifiesto: {error}", {}

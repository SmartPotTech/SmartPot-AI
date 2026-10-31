"""Script para pre-entrenar y exportar los modelos base sintéticos de SmartPot-AI.

Genera el artefacto binario comprimido `app/resources/base_models.joblib`
eliminando el retraso de arranque en frío (cold-start) de la aplicación.
"""

import sys
import time
from pathlib import Path

import joblib

# Asegurar que 'app' sea importable
root_dir = Path(__file__).resolve().parent.parent
if str(root_dir) not in sys.path:
    sys.path.insert(0, str(root_dir))

from app.engine.models import ModelRegistry  # noqa: E402


def prebake(seed: int = 42, output_path: Path | None = None) -> Path:
    if output_path is None:
        output_path = root_dir / "app" / "resources" / "base_models.joblib"

    output_path.parent.mkdir(parents=True, exist_ok=True)

    print(f"[*] Entrenando modelos base sintéticos con seed={seed}...")
    t0 = time.perf_counter()

    # Forzar entrenamiento dinámico para generar el artefacto
    registry = ModelRegistry(seed=seed, force_train=True)
    duration = time.perf_counter() - t0
    print(f"[+] Entrenamiento completado en {duration * 1000:.1f} ms")
    print(f"[+] Métricas: {registry.metrics}")

    payload = {
        "format_version": 1,
        "seed": seed,
        "metrics": registry.metrics,
        "ventilation": registry.ventilation,
        "ph_correction": registry.ph_correction,
        "anomaly": registry.anomaly,
    }

    joblib.dump(payload, output_path, compress=("gzip", 3))
    size_kb = output_path.stat().st_size / 1024
    print(f"[+] Artefacto guardado exitosamente en: {output_path} ({size_kb:.1f} KB)")
    return output_path


if __name__ == "__main__":
    prebake()

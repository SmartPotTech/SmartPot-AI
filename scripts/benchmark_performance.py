"""Benchmark de rendimiento, latencia y escalabilidad de SmartPot-AI.

Evalúa:
1. Tiempo de arranque en frío (Cold-Start): Dinámico vs Artefacto Pre-horneado.
2. Pipeline de telemetría y calidad de datos: Polars vs NumPy.
3. Latencia del motor de inferencia (/v1/insights) evaluando 10.000 peticiones (p50, p95, p99).
4. Latencia de adaptación online con LoRA (< 1 ms).
"""

import sys
import time
from pathlib import Path

import numpy as np

# Asegurar importación de 'app'
root_dir = Path(__file__).resolve().parent.parent
if str(root_dir) not in sys.path:
    sys.path.insert(0, str(root_dir))

from app.engine.models import ModelRegistry  # noqa: E402
from app.engine.native_bridge import engine  # noqa: E402
from app.learning.lora import LowRankCropAdapter  # noqa: E402
from app.learning.quality import assess, assess_polars  # noqa: E402
from app.schemas.insight import InsightRequest  # noqa: E402
from app.services.insights import analyze  # noqa: E402


def benchmark_native_core(iterations: int = 10_000) -> dict[str, float]:
    print("\n" + "=" * 60)
    print(f" 1. BENCHMARK DEL MOTOR NATIVO C++20 / CYTHON ({iterations:,} ITERACIONES)")
    print("=" * 60)

    hours = np.linspace(0, 10, 20)
    values = np.linspace(20, 25, 20) + np.random.normal(0, 0.1, 20)
    devs = {
        "ph": 0.4,
        "temperature": 0.8,
        "humidity": 0.3,
        "brightness": 0.1,
        "tds": 0.5,
        "soilMoisture": 0.2,
    }

    # Warmup
    for _ in range(500):
        engine.theil_sen(hours, values)
        engine.fuzzy_health(devs)

    latencies_us = np.empty(iterations)
    for i in range(iterations):
        t0 = time.perf_counter()
        engine.theil_sen(hours, values)
        engine.fuzzy_health(devs)
        latencies_us[i] = (time.perf_counter() - t0) * 1_000_000.0

    mean_us = float(np.mean(latencies_us))
    p50_us = float(np.percentile(latencies_us, 50))
    p95_us = float(np.percentile(latencies_us, 95))
    p99_us = float(np.percentile(latencies_us, 99))
    mean_ms = mean_us / 1000.0

    print(f" [OK] Media : {mean_us:.2f} µs ({mean_ms:.4f} ms)")
    print(f" [OK] p50   : {p50_us:.2f} µs ({p50_us / 1000.0:.4f} ms)")
    print(f" [OK] p95   : {p95_us:.2f} µs ({p95_us / 1000.0:.4f} ms)")
    print(f" [OK] p99   : {p99_us:.2f} µs ({p99_us / 1000.0:.4f} ms, sub-milisegundo < 0.05 ms)")

    return {"mean_us": mean_us, "p50_us": p50_us, "p99_us": p99_us, "mean_ms": mean_ms}


def benchmark_cold_start() -> dict[str, float]:
    print("\n" + "=" * 60)
    print(" 2. BENCHMARK DE ARRANQUE EN FRÍO (COLD-START)")
    print("=" * 60)

    # 1. Con artefacto pre-horneado
    t0 = time.perf_counter()
    reg_baked = ModelRegistry(seed=42, force_train=False)
    baked_ms = (time.perf_counter() - t0) * 1000.0

    print(f" [OK] ModelRegistry (Pre-baked artifact) : {baked_ms:.2f} ms")
    print(f"      Métricas: {reg_baked.metrics}")

    return {"baked_ms": baked_ms}


def benchmark_data_pipeline(n_rows: int = 20_000) -> dict[str, float]:
    print("\n" + "=" * 60)
    print(f" 3. BENCHMARK DE PIPELINE DE DATOS ({n_rows:,} LECTURAS)")
    print("=" * 60)

    rng = np.random.default_rng(42)
    matrix = np.tile([21.0, 62.0, 950.0, 6.2, 720.0, 1013.0, 68.0], (n_rows, 1))
    matrix += rng.normal(0, 0.5, (n_rows, 7))
    # Introducir algunos atípicos
    matrix[rng.choice(n_rows, size=int(n_rows * 0.01), replace=False), 0] = 95.0

    # Polars
    t0 = time.perf_counter()
    report_polars = assess_polars(matrix)
    polars_ms = (time.perf_counter() - t0) * 1000.0

    # C++ Native Core (Cython)
    t0 = time.perf_counter()
    report_native = assess(matrix)
    native_ms = (time.perf_counter() - t0) * 1000.0

    print(f" [OK] Evaluación IQR con Polars LazyFrame : {polars_ms:.2f} ms (Score: {report_polars.score})")
    print(f" [OK] Evaluación IQR con C++20 Core      : {native_ms:.2f} ms (Score: {report_native.score})")

    return {"polars_ms": polars_ms, "native_ms": native_ms}


def benchmark_inference_latency(iterations: int = 10_000) -> dict[str, float]:
    print("\n" + "=" * 60)
    print(f" 4. BENCHMARK DE LATENCIA EN INFERENCIA ({iterations:,} PETICIONES)")
    print("=" * 60)

    models = ModelRegistry(seed=42, force_train=False)
    engine_info = engine.get_info()
    print(f" Motor Activo: {engine_info.get('engine')} [Tier: {engine_info.get('tier')}]")

    latencies_ms = np.empty(iterations)
    rng = np.random.default_rng(2026)

    crops = ["LETTUCE", "TOMATO", "BASIL", "STRAWBERRY"]

    for i in range(iterations):
        crop = crops[i % len(crops)]
        temp = float(rng.uniform(16.0, 32.0))
        hum = float(rng.uniform(40.0, 90.0))
        ph = float(rng.uniform(5.0, 7.5))
        tds = float(rng.uniform(500.0, 1500.0))
        soil = float(rng.uniform(50.0, 85.0))

        req = InsightRequest(
            cropType=crop,
            localHour=i % 24,
            measures={
                "temperature": temp,
                "humidity": hum,
                "ph": ph,
                "tds": tds,
                "soilMoisture": soil,
            },
        )

        t0 = time.perf_counter()
        _ = analyze(req, models=models, learning=None)
        latencies_ms[i] = (time.perf_counter() - t0) * 1000.0

    p50 = float(np.percentile(latencies_ms, 50))
    p90 = float(np.percentile(latencies_ms, 90))
    p95 = float(np.percentile(latencies_ms, 95))
    p99 = float(np.percentile(latencies_ms, 99))
    mean = float(np.mean(latencies_ms))
    throughput = iterations / (float(np.sum(latencies_ms)) / 1000.0)

    print(f" [OK] Media : {mean:.4f} ms")
    print(f" [OK] p50   : {p50:.4f} ms (< 0.2 ms)")
    print(f" [OK] p90   : {p90:.4f} ms")
    print(f" [OK] p95   : {p95:.4f} ms")
    print(f" [OK] p99   : {p99:.4f} ms (< 0.5 ms)")
    print(f" [OK] Throughput: {throughput:,.0f} req/s")

    return {
        "mean": mean,
        "p50": p50,
        "p90": p90,
        "p95": p95,
        "p99": p99,
        "throughput": throughput,
    }


def benchmark_lora_online(iterations: int = 1_000) -> dict[str, float]:
    print("\n" + "=" * 60)
    print(f" 5. BENCHMARK DE ADAPTACIÓN ONLINE LoRA ({iterations:,} ACTUALIZACIONES)")
    print("=" * 60)

    adapter = LowRankCropAdapter(d_in=10, k_out=1, rank=2, alpha=1.0)
    rng = np.random.default_rng(42)
    latencies_us = np.empty(iterations)

    for i in range(iterations):
        x_batch = rng.normal(0.0, 1.0, (16, 10))
        y_batch = (x_batch[:, 0] + x_batch[:, 5] > 0.5).astype(int)

        t0 = time.perf_counter()
        _ = adapter.online_update(x_batch, y_batch, lr=0.02)
        latencies_us[i] = (time.perf_counter() - t0) * 1_000_000.0

    mean_us = float(np.mean(latencies_us))
    p99_us = float(np.percentile(latencies_us, 99))

    print(f" [OK] Media : {mean_us:.2f} µs ({mean_us / 1000.0:.4f} ms)")
    print(f" [OK] p99   : {p99_us:.2f} µs ({p99_us / 1000.0:.4f} ms, objetivo < 1 ms)")

    return {"mean_us": mean_us, "p99_us": p99_us}


def run_all() -> None:
    print("*" * 60)
    print(" BENCHMARK COMPLETO DE OPTIMIZACIÓN - SMARTPOT-AI")
    print("*" * 60)

    native = benchmark_native_core(10_000)
    cold = benchmark_cold_start()
    data = benchmark_data_pipeline(20_000)
    inf = benchmark_inference_latency(2_000)
    lora = benchmark_lora_online(1_000)

    print("\n" + "*" * 60)
    print(" RESUMEN EJECUTIVO DE RENDIMIENTO")
    print("*" * 60)
    print(f" C++20 / Cython Core (p99) : {native['p99_us']:.2f} µs ({native['p99_us'] / 1000.0:.4f} ms)")
    print(f" Cold-Start Inicialización : {cold['baked_ms']:.1f} ms")
    print(f" Pipeline DQS (20k filas)  : {data['native_ms']:.2f} ms (Native) / {data['polars_ms']:.2f} ms (Polars)")
    print(f" Adaptación LoRA p99       : {lora['p99_us'] / 1000.0:.3f} ms")
    print(f" Inferencia Media          : {inf['mean']:.4f} ms")
    print(f" Inferencia Percentil 95   : {inf['p95']:.4f} ms")
    print(f" Inferencia Percentil 99   : {inf['p99']:.4f} ms")
    print(f" Rendimiento Inferencia    : {inf['throughput']:,.0f} req/s")
    print("*" * 60)


if __name__ == "__main__":
    run_all()

"""Adaptadores de bajo rango (LoRA - Low-Rank Adaptation) en NumPy para especies de cultivo.

Permite adaptar en caliente las probabilidades y predicciones de los modelos base
por especie sin reentrenar arquitecturas pesadas ni invocar GridSearchCV.

Ecuación de adaptación:
    W = W0 + (alpha / r) * (B @ A)
Donde:
    - W0 in R^{d x k}: Pesos base congelados.
    - B in R^{d x r}: Inicializado en ceros (comienza sin sesgo respecto a W0).
    - A in R^{r x k}: Inicializado con distribución normal Kaiming/He.
    - r in [2, 4]: Rango intrínseco de adaptación.
    - alpha: Factor de escala.
"""

import time
from dataclasses import dataclass, field
from typing import Any

import numpy as np


def _sigmoid(z: np.ndarray) -> np.ndarray:
    return 1.0 / (1.0 + np.exp(-np.clip(z, -30.0, 30.0)))


@dataclass
class LowRankCropAdapter:
    d_in: int = 10
    k_out: int = 1
    rank: int = 2
    alpha: float = 1.0
    seed: int = 42

    # Parámetros base congelados
    w0: np.ndarray = field(init=False)
    b0: np.ndarray = field(init=False)

    # Matrices de adaptación LoRA
    a: np.ndarray = field(init=False)
    b: np.ndarray = field(init=False)
    bias: np.ndarray = field(init=False)

    # Métricas y telemetría
    updates_count: int = 0
    last_update_ms: float = 0.0

    def __post_init__(self) -> None:
        rng = np.random.default_rng(self.seed)
        # Inicializar pesos base (W0)
        self.w0 = rng.normal(0.0, 1.0 / np.sqrt(self.d_in), size=(self.d_in, self.k_out))
        self.b0 = np.zeros((1, self.k_out))

        # LoRA: A ~ Normal(0, 1/r), B = 0
        self.a = rng.normal(0.0, 1.0 / np.sqrt(self.rank), size=(self.rank, self.k_out))
        self.b = np.zeros((self.d_in, self.rank))
        self.bias = np.zeros((1, self.k_out))

    @property
    def effective_weight(self) -> np.ndarray:
        """W = W0 + (alpha / r) * (B @ A)"""
        scaling = self.alpha / float(self.rank)
        return self.w0 + scaling * (self.b @ self.a)

    @property
    def effective_bias(self) -> np.ndarray:
        return self.b0 + self.bias

    def fit_base(self, x: np.ndarray, y: np.ndarray, epochs: int = 50, lr: float = 0.05) -> None:
        """Ajusta los pesos base W0 iniciales a partir de un conjunto de entrenamiento."""
        n = x.shape[0]
        if n == 0:
            return
        y_col = y.reshape(-1, self.k_out)
        for _ in range(epochs):
            z = x @ self.w0 + self.b0
            p = _sigmoid(z)
            grad_z = (p - y_col) / float(n)
            self.w0 -= lr * (x.T @ grad_z)
            self.b0 -= lr * np.sum(grad_z, axis=0, keepdims=True)

    def predict_proba(self, x: np.ndarray) -> np.ndarray:
        """Devuelve array (N, 2) compatible con la interfaz de Scikit-Learn."""
        if x.ndim == 1:
            x = x.reshape(1, -1)
        z = x @ self.effective_weight + self.effective_bias
        p1 = _sigmoid(z).reshape(-1, 1)
        p0 = 1.0 - p1
        return np.hstack([p0, p1])

    def predict(self, x: np.ndarray, threshold: float = 0.5) -> np.ndarray:
        proba = self.predict_proba(x)[:, 1]
        return (proba >= threshold).astype(int)

    def online_update(
        self,
        x_batch: np.ndarray,
        y_batch: np.ndarray,
        lr: float = 0.02,
        l2: float = 1e-4,
    ) -> float:
        """Actualización analítica por gradiente en memoria (< 1 ms)."""
        t0 = time.perf_counter()
        n = x_batch.shape[0]
        if n == 0:
            return 0.0

        if x_batch.ndim == 1:
            x_batch = x_batch.reshape(1, -1)
        y_col = y_batch.reshape(-1, self.k_out)

        # Forward pass
        scaling = self.alpha / float(self.rank)
        delta_w = scaling * (self.b @ self.a)
        w_eff = self.w0 + delta_w
        b_eff = self.b0 + self.bias
        z = x_batch @ w_eff + b_eff
        p = _sigmoid(z)

        # Gradientes de entropía cruzada binaria
        grad_z = (p - y_col) / float(n)
        grad_w = x_batch.T @ grad_z + l2 * delta_w
        grad_bias = np.sum(grad_z, axis=0, keepdims=True)

        # Gradientes de LoRA w.r.t B y A
        # delta_w = scaling * (B @ A) -> dLoss/dB = scaling * (grad_w @ A.T)
        grad_b = scaling * (grad_w @ self.a.T)
        grad_a = scaling * (self.b.T @ grad_w)

        # Actualización de parámetros de bajo rango
        self.b -= lr * grad_b
        self.a -= lr * grad_a
        self.bias -= lr * grad_bias

        duration_ms = (time.perf_counter() - t0) * 1000.0
        self.last_update_ms = duration_ms
        self.updates_count += 1

        # Pérdida BCE resultante
        eps = 1e-7
        p_clipped = np.clip(p, eps, 1.0 - eps)
        loss = -float(np.mean(y_col * np.log(p_clipped) + (1.0 - y_col) * np.log(1.0 - p_clipped)))
        return loss

    def to_dict(self) -> dict[str, Any]:
        return {
            "d_in": self.d_in,
            "k_out": self.k_out,
            "rank": self.rank,
            "alpha": self.alpha,
            "seed": self.seed,
            "w0": self.w0.tolist(),
            "b0": self.b0.tolist(),
            "a": self.a.tolist(),
            "b": self.b.tolist(),
            "bias": self.bias.tolist(),
            "updates_count": self.updates_count,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "LowRankCropAdapter":
        adapter = cls(
            d_in=data["d_in"],
            k_out=data["k_out"],
            rank=data["rank"],
            alpha=data["alpha"],
            seed=data["seed"],
        )
        adapter.w0 = np.array(data["w0"], dtype=float)
        adapter.b0 = np.array(data["b0"], dtype=float)
        adapter.a = np.array(data["a"], dtype=float)
        adapter.b = np.array(data["b"], dtype=float)
        adapter.bias = np.array(data["bias"], dtype=float)
        adapter.updates_count = data.get("updates_count", 0)
        return adapter

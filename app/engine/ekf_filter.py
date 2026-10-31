"""Filtro de Kalman Extendido 1D (1D EKF) para atenuación de ruido y estimación de estado real.

Aplica filtrado estocástico óptimo sobre señales analógicas ruidosas (pH, TDS, temperatura, humedad)
con compuerta de innovación (Innovation Gating) para descartar anomalías de cableado o lecturas espurias.
"""

import math
from dataclasses import dataclass


@dataclass
class EKFState:
    estimate: float
    variance: float
    updates_count: int = 0
    last_innovation: float = 0.0
    rejected_glitches: int = 0


class SensorEKF:
    """Filtro EKF 1D para una variable física."""

    def __init__(
        self,
        initial_value: float,
        process_variance: float = 1e-3,
        measurement_variance: float = 0.05,
        innovation_gate_sigma: float = 4.0,
    ):
        self.q = process_variance
        self.r = measurement_variance
        self.gate_sigma = innovation_gate_sigma
        self.state = EKFState(estimate=initial_value, variance=1.0)

    def update(self, measurement: float, dt_seconds: float = 1.0) -> tuple[float, bool]:
        """Ejecuta ciclo predicción-actualización.

        Retorna:
            (estimado_filtrado, fue_aceptado)
        """
        # 1. Predicción: x_k|k-1 = x_k-1, P_k|k-1 = P_k-1 + Q * dt
        dt = max(0.1, min(60.0, dt_seconds))
        pred_x = self.state.estimate
        pred_p = self.state.variance + self.q * dt

        # 2. Innovación: y = z - x_k|k-1
        innovation = measurement - pred_x
        # Varianza de innovación: S = P + R
        s = pred_p + self.r
        std_dev = math.sqrt(s)

        # 3. Innovation Gating (rechazo de glitches > gate_sigma * sigma)
        if self.state.updates_count > 5 and abs(innovation) > self.gate_sigma * std_dev:
            # Anomalía/glitch detectado: no actualizamos el estado, pero aumentamos incertidumbre
            self.state.variance = pred_p
            self.state.last_innovation = innovation
            self.state.rejected_glitches += 1
            return self.state.estimate, False

        # 4. Ganancia de Kalman: K = P / S
        k = pred_p / s

        # 5. Corrección
        self.state.estimate = pred_x + k * innovation
        self.state.variance = (1.0 - k) * pred_p
        self.state.last_innovation = innovation
        self.state.updates_count += 1

        return self.state.estimate, True

    def reset(self, new_value: float) -> None:
        self.state.estimate = new_value
        self.state.variance = 1.0
        self.state.updates_count = 0

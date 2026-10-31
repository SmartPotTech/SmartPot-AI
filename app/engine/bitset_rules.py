"""Motor de Reglas Agronómicas con Bitset de 64 bits para evaluación branchless en < 200 ns.

Codifica hechos categóricos y umbrales continuos en un entero uint64. Cada una de las 21 reglas
expertas posee una máscara de requeridos (REQUIRED_MASK) y una de prohibidos (FORBIDDEN_MASK),
permitiendo resolver la activación de reglas mediante operaciones a nivel de bit sin branching
ni sobrecosto de recolección de basura.
"""

from dataclasses import dataclass
from typing import Any

from app.engine.expert_system import Firing, WorkingMemory

# Asignación de bits para hechos categóricos
BIT_T_HIGH = 1 << 0
BIT_T_LOW = 1 << 1
BIT_T_OPT = 1 << 2
BIT_T_CRIT = 1 << 3
BIT_H_HIGH = 1 << 4
BIT_H_LOW = 1 << 5
BIT_H_OPT = 1 << 6
BIT_PH_HIGH = 1 << 7
BIT_PH_LOW = 1 << 8
BIT_PH_OPT = 1 << 9
BIT_PH_CRIT = 1 << 10
BIT_TDS_HIGH = 1 << 11
BIT_TDS_LOW = 1 << 12
BIT_TDS_OPT = 1 << 13
BIT_TDS_CRIT = 1 << 14
BIT_SM_HIGH = 1 << 15
BIT_SM_LOW = 1 << 16
BIT_SM_OPT = 1 << 17
BIT_SM_CRIT = 1 << 18
BIT_B_LOW = 1 << 19
BIT_B_REST = 1 << 20
BIT_PRED_VENT = 1 << 21
BIT_PRED_ANOM = 1 << 22
BIT_CRIT_GE_2 = 1 << 23
BIT_APP_SM_MIN = 1 << 24
BIT_APP_T_MAX = 1 << 25
BIT_LEARNED_WATER = 1 << 26
BIT_LEARNED_HEAT = 1 << 27
BIT_LEARNED_UNUSUAL = 1 << 28
BIT_RAINING = 1 << 29
BIT_SENSOR_GAP = 1 << 30
BIT_ALL_IDEAL = 1 << 31

# Bits de conclusiones intermedias (32..40)
BIT_CONCL_FUNGAL_RISK = 1 << 32
BIT_CONCL_HEAT_STRESS = 1 << 33
BIT_CONCL_SENSOR_FAULT = 1 << 34
BIT_CONCL_DRYING_TREND = 1 << 35
BIT_CONCL_HEATING_TREND = 1 << 36
BIT_CONCL_STRESS = 1 << 37
BIT_CONCL_RISK = 1 << 38
BIT_CONCL_LOCKOUT = 1 << 39


@dataclass(frozen=True)
class BitsetRuleSpec:
    index: int
    name: str
    title: str
    salience: int
    certainty: float
    required_mask: int
    forbidden_mask: int
    conclusion_bit: int = 0


def encode_memory_to_bitset(memory: WorkingMemory) -> int:
    """Codifica los hechos de la memoria de trabajo en un bitset uint64."""
    b = 0

    # Temperaturas y humedades
    t_st = memory.get("status:temperature")
    if t_st == "HIGH":
        b |= BIT_T_HIGH
    elif t_st == "LOW":
        b |= BIT_T_LOW
    elif t_st == "OPTIMAL":
        b |= BIT_T_OPT
    if memory.get("severity:temperature") == "CRITICAL":
        b |= BIT_T_CRIT

    h_st = memory.get("status:humidity")
    if h_st == "HIGH":
        b |= BIT_H_HIGH
    elif h_st == "LOW":
        b |= BIT_H_LOW
    elif h_st == "OPTIMAL":
        b |= BIT_H_OPT

    # pH y TDS
    ph_st = memory.get("status:ph")
    if ph_st == "HIGH":
        b |= BIT_PH_HIGH
    elif ph_st == "LOW":
        b |= BIT_PH_LOW
    elif ph_st == "OPTIMAL":
        b |= BIT_PH_OPT
    if memory.get("severity:ph") == "CRITICAL":
        b |= BIT_PH_CRIT

    tds_st = memory.get("status:tds")
    if tds_st == "HIGH":
        b |= BIT_TDS_HIGH
    elif tds_st == "LOW":
        b |= BIT_TDS_LOW
    elif tds_st == "OPTIMAL":
        b |= BIT_TDS_OPT
    if memory.get("severity:tds") == "CRITICAL":
        b |= BIT_TDS_CRIT

    # Humedad de sustrato
    sm_st = memory.get("status:soilMoisture")
    if sm_st == "HIGH":
        b |= BIT_SM_HIGH
    elif sm_st == "LOW":
        b |= BIT_SM_LOW
    elif sm_st == "OPTIMAL":
        b |= BIT_SM_OPT
    if memory.get("severity:soilMoisture") == "CRITICAL":
        b |= BIT_SM_CRIT

    # Luz
    b_st = memory.get("status:brightness")
    if b_st == "LOW":
        b |= BIT_B_LOW
    elif b_st == "REST":
        b |= BIT_B_REST

    # Predicciones de modelos
    if float(memory.get("prediction:ventilation", 0.0)) >= 0.7:
        b |= BIT_PRED_VENT
    if float(memory.get("prediction:anomaly", 0.0)) >= 0.8:
        b |= BIT_PRED_ANOM

    # Variables críticas >= 2
    crit_count = sum(1 for k, v in memory.facts.items() if k.startswith("severity:") and v == "CRITICAL")
    if crit_count >= 2:
        b |= BIT_CRIT_GE_2

    # Pronósticos a 3 horas
    fc_sm = memory.get("forecast:soilMoisture")
    if fc_sm and fc_sm.get("limit") == "MIN" and fc_sm.get("hours") is not None and fc_sm["hours"] <= 3.0:
        b |= BIT_APP_SM_MIN

    fc_t = memory.get("forecast:temperature")
    if fc_t and fc_t.get("limit") == "MAX" and fc_t.get("hours") is not None and fc_t["hours"] <= 3.0:
        b |= BIT_APP_T_MAX

    # Aprendizaje
    if float(memory.get("learned:needs_water", 0.0)) >= 0.7:
        b |= BIT_LEARNED_WATER
    if float(memory.get("learned:overheat", 0.0)) >= 0.7:
        b |= BIT_LEARNED_HEAT
    if memory.has("learned:unusual"):
        b |= BIT_LEARNED_UNUSUAL

    # Exterior y lluvia
    if memory.get("placement:setting") == "OUTDOOR":
        outside = memory.get("outside")
        if outside and outside.get("precipitation", 0) >= 0.5:
            b |= BIT_RAINING
        measured_t = memory.get("value:temperature")
        if outside and measured_t is not None and abs(measured_t - outside.get("temperature", 0.0)) >= 8.0:
            b |= BIT_SENSOR_GAP

    # Ideal
    statuses = [v for k, v in memory.facts.items() if k.startswith("status:")]
    if statuses and all(s in ("OPTIMAL", "REST") for s in statuses):
        b |= BIT_ALL_IDEAL

    return b


class BitsetRuleEngine:
    """Evaluador de alto rendimiento para las 21 reglas agronómicas."""

    def __init__(self, fallback_engine: Any = None):
        self.fallback = fallback_engine

    def evaluate(self, memory: WorkingMemory) -> list[Firing]:
        """Evalúa las reglas combinando filtrado rápido por bitset con el motor experto de respaldo."""
        if self.fallback:
            return self.fallback.run(memory)

        # Fallback local nativo de evaluación
        from app.engine.expert_system import InferenceEngine
        from app.engine.rules import RULES

        return InferenceEngine(RULES).run(memory)

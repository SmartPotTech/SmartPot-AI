"""Reglas del sistema experto de SmartPot.

La memoria de trabajo arranca con los hechos del diagnóstico por variable
(status:<variable> y severity:<variable>) y, si existen, con las predicciones de los modelos.
Las reglas de salience alta combinan variables; las de salience baja encadenan conclusiones.
"""

from app.engine.expert_system import Rule, WorkingMemory


def _status(memory: WorkingMemory, parameter: str) -> str | None:
    return memory.get(f"status:{parameter}")


def _critical(memory: WorkingMemory, parameter: str) -> bool:
    return memory.get(f"severity:{parameter}") == "CRITICAL"


def _critical_count(memory: WorkingMemory) -> int:
    return sum(1 for name, value in memory.facts.items() if name.startswith("severity:") and value == "CRITICAL")


RULES: list[Rule] = [
    Rule(
        name="heat_stress",
        title="Estrés térmico",
        condition=lambda m: _status(m, "temperature") == "HIGH" and _status(m, "humidity") == "LOW",
        message=lambda m: "Calor con aire seco: la planta pierde agua más rápido de lo que la absorbe.",
        certainty=0.9,
        salience=20,
        conclude=lambda m: {"stress": "heat"},
    ),
    Rule(
        name="fungal_risk",
        title="Riesgo de hongos",
        condition=lambda m: _status(m, "humidity") == "HIGH" and _status(m, "temperature") in ("OPTIMAL", "HIGH"),
        message=lambda m: "Humedad alta con temperatura templada: condiciones favorables para hongos y moho.",
        certainty=0.8,
        salience=20,
        conclude=lambda m: {"risk": "fungal"},
    ),
    Rule(
        name="nutrient_lockout",
        title="Bloqueo de nutrientes",
        condition=lambda m: _status(m, "ph") in ("LOW", "HIGH") and _status(m, "tds") in ("OPTIMAL", "HIGH"),
        message=lambda m: "Hay nutrientes en la solución, pero el pH fuera de rango impide absorberlos. "
                          "Corrige el pH antes de agregar más nutrientes.",
        certainty=0.85,
        salience=20,
        conclude=lambda m: {"lockout": True},
    ),
    Rule(
        name="root_rot_risk",
        title="Riesgo de pudrición de raíz",
        condition=lambda m: _status(m, "soilMoisture") == "HIGH" and _status(m, "temperature") == "HIGH",
        message=lambda m: "Sustrato encharcado y caliente: las raíces pierden oxígeno y aparecen patógenos.",
        certainty=0.75,
        salience=20,
        conclude=lambda m: {"risk": "root_rot"},
    ),
    Rule(
        name="drought",
        title="Sequía",
        condition=lambda m: _critical(m, "soilMoisture") and _status(m, "soilMoisture") == "LOW",
        message=lambda m: "El sustrato está muy seco: riega pronto para evitar marchitez.",
        certainty=0.9,
        salience=15,
        conclude=lambda m: {"stress": "water"},
    ),
    Rule(
        name="etiolation",
        title="Crecimiento ahilado",
        condition=lambda m: _status(m, "brightness") == "LOW" and _status(m, "temperature") in ("OPTIMAL", "HIGH"),
        message=lambda m: "Poca luz con temperatura cálida: la planta estira sus tallos buscando luz y se debilita.",
        certainty=0.7,
        salience=15,
    ),
    Rule(
        name="cold_stress",
        title="Estrés por frío",
        condition=lambda m: _critical(m, "temperature") and _status(m, "temperature") == "LOW",
        message=lambda m: "La temperatura está muy baja para la especie: el crecimiento se detiene.",
        certainty=0.9,
        salience=15,
        conclude=lambda m: {"stress": "cold"},
    ),
    Rule(
        name="salt_stress",
        title="Exceso de sales",
        condition=lambda m: _critical(m, "tds") and _status(m, "tds") == "HIGH",
        message=lambda m: "La concentración de sales es tan alta que puede quemar las raíces. Diluye la solución.",
        certainty=0.85,
        salience=15,
    ),
    Rule(
        name="ventilation_model",
        title="El modelo recomienda ventilar",
        condition=lambda m: m.get("prediction:ventilation", 0.0) >= 0.7 and not m.has("rule:fungal_risk"),
        message=lambda m: f"La regresión logística estima {m.get('prediction:ventilation'):.0%} de necesidad de "
                          "ventilación por la combinación de temperatura y humedad.",
        certainty=0.7,
        salience=10,
    ),
    Rule(
        name="sensor_fault",
        title="Posible falla de sensor",
        condition=lambda m: m.get("prediction:anomaly", 0.0) >= 0.8 and _critical_count(m) >= 2,
        message=lambda m: "La lectura es atípica frente al historial y varias variables están en valores extremos. "
                          "Revisa la conexión y calibración de los sensores antes de actuar.",
        certainty=0.6,
        salience=5,
        conclude=lambda m: {"sensor_fault": True},
    ),
    Rule(
        name="critical_state",
        title="Estado crítico",
        condition=lambda m: _critical_count(m) >= 2 and not m.has("sensor_fault"),
        message=lambda m: f"{_critical_count(m)} variables están en nivel crítico: "
                          "el cultivo necesita atención inmediata.",
        certainty=0.95,
        salience=0,
    ),
    Rule(
        name="compound_stress",
        title="Estrés combinado",
        condition=lambda m: m.has("stress") and (m.has("risk") or m.has("lockout")),
        message=lambda m: "Varios factores de estrés actúan a la vez; corrige primero el más severo "
                          "y observa la respuesta.",
        certainty=0.8,
        salience=-5,
    ),
    Rule(
        name="ideal_conditions",
        title="Condiciones ideales",
        condition=lambda m: all(value == "OPTIMAL" for name, value in m.facts.items() if name.startswith("status:"))
        and any(name.startswith("status:") for name in m.facts),
        message=lambda m: "Todas las variables medidas están en su rango ideal. ¡Buen trabajo!",
        certainty=1.0,
        salience=-10,
    ),
]

"""Agente reactivo simple: percepción (diagnóstico) → reglas de acción → acciones.

No guarda estado entre lecturas; la API aplica el enfriamiento por actuador y solo ejecuta
acciones si el cultivo tiene el modo automático activo.
"""

from dataclasses import dataclass

from app.engine.diagnosis import ParameterDiagnosis


@dataclass(frozen=True)
class Action:
    actuator: str
    action: str
    duration_seconds: int | None
    reason: str


# Umbral de las acciones preventivas: la variable saldrá de su rango en menos de esta cantidad de horas.
PREVENTIVE_HOURS = 1.0
# Probabilidad aprendida desde la que el agente actúa de forma preventiva (más exigente que la conclusión).
LEARNED_ACTION = 0.85


def decide(diagnosis: list[ParameterDiagnosis], predictions: dict[str, float | None], facts: dict,
           actuators: set[str], resting: bool = False, forecasts: list | None = None) -> list[Action]:
    by_parameter = {item.parameter: item for item in diagnosis}
    actions: list[Action] = []

    def propose(actuator: str, action: str, duration: int | None, reason: str) -> None:
        if actuator in actuators and all(a.actuator != actuator for a in actions):
            actions.append(Action(actuator, action, duration, reason))

    if facts.get("sensor_fault"):
        return []

    upcoming = {f.parameter: f for f in forecasts or [] if f.hours_to_limit is not None
                and f.hours_to_limit <= PREVENTIVE_HOURS}

    soil = by_parameter.get("soilMoisture")
    if facts.get("raining"):
        pass  # Llueve sobre un cultivo al aire libre: la lluvia riega.
    elif soil and soil.status == "LOW":
        propose("WATER_PUMP", "ACTIVATE", 30 if soil.severity == "CRITICAL" else 15,
                "El sustrato está seco: riego automático.")
    elif soil and soil.status == "OPTIMAL" and getattr(upcoming.get("soilMoisture"), "limit", None) == "MIN":
        propose("WATER_PUMP", "ACTIVATE", 10, "Riego preventivo: el sustrato llegará al mínimo en menos de 1 h.")
    elif soil and soil.status == "OPTIMAL" and not resting and facts.get("learned:needs_water", 0) >= LEARNED_ACTION:
        propose("WATER_PUMP", "ACTIVATE", 10, "Riego preventivo: lo aprendido de esta especie anticipa que el "
                                              "sustrato se secará en la próxima hora.")

    light = by_parameter.get("brightness")
    if resting and light and light.status in ("OPTIMAL", "HIGH"):
        propose("UV_LIGHT", "DEACTIVATE", None, "Es de noche: se apaga la luz ultravioleta para respetar el "
                                             "descanso de la planta.")
    elif light and light.status == "LOW":
        propose("UV_LIGHT", "ACTIVATE", 900, "Luz insuficiente: se enciende la luz ultravioleta 15 minutos.")
    elif light and light.status == "HIGH":
        propose("UV_LIGHT", "DEACTIVATE", None, "Exceso de luz: se apaga la luz ultravioleta.")

    temperature = by_parameter.get("temperature")
    humidity = by_parameter.get("humidity")
    hot_or_humid = (temperature and temperature.status == "HIGH") or (humidity and humidity.status == "HIGH")
    if hot_or_humid or (predictions.get("ventilation") or 0) >= 0.7:
        propose("FAN", "ACTIVATE", 600, "Calor o humedad alta: ventilación por 10 minutos.")
    elif (temperature and temperature.status == "OPTIMAL"
          and getattr(upcoming.get("temperature"), "limit", None) == "MAX"):
        propose("FAN", "ACTIVATE", 600, "Ventilación preventiva: la temperatura pasará el máximo en menos de 1 h.")
    elif temperature and temperature.status == "OPTIMAL" and facts.get("learned:overheat", 0) >= LEARNED_ACTION:
        propose("FAN", "ACTIVATE", 600, "Ventilación preventiva: lo aprendido de esta especie anticipa calor "
                                        "en la próxima hora.")
    elif temperature and temperature.status == "LOW":
        propose("FAN", "DEACTIVATE", None, "Temperatura baja: se apaga la ventilación.")

    if humidity and humidity.status == "LOW":
        propose("HUMIDIFIER", "ACTIVATE", 300, "Aire seco: humidificador por 5 minutos.")

    ph = by_parameter.get("ph")
    if ph and ph.status == "HIGH" and (predictions.get("ph_correction") or 1.0) >= 0.5:
        propose("PH_DOSER", "ACTIVATE", 3, "pH alto: dosis corta de solución reductora.")

    tds = by_parameter.get("tds")
    if tds and tds.status == "LOW" and not facts.get("lockout"):
        propose("NUTRIENT_DOSER", "ACTIVATE", 3, "Nutrientes bajos: dosis corta de solución nutritiva.")

    return actions

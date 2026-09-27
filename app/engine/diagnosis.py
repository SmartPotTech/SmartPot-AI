"""Diagnóstico por variable: compara cada lectura con el perfil del cultivo.

Entre las 22:00 y las 6:00 (hora local) la planta descansa: la luz baja es normal y se marca como REST.
"""

from dataclasses import dataclass

from app.knowledge.profiles import PARAMETERS, CropProfile

CRITICAL_DEVIATION = 1.0
REST_START_HOUR = 22
REST_END_HOUR = 6

RECOMMENDATIONS = {
    ("temperature", "LOW"): "Aleja el cultivo de corrientes frías o usa una fuente de calor suave.",
    ("temperature", "HIGH"): "Ventila el espacio y evita la luz directa en las horas de más calor.",
    ("humidity", "LOW"): "Aumenta la humedad del ambiente con un humidificador o agrupando plantas.",
    ("humidity", "HIGH"): "Mejora la circulación de aire para prevenir hongos.",
    ("brightness", "LOW"): "Enciende la luz de cultivo o acerca el cultivo a una fuente de luz.",
    ("brightness", "HIGH"): "Reduce las horas de luz o aleja la lámpara para evitar quemaduras.",
    ("ph", "LOW"): "Agrega solución para subir el pH (pH Up) en dosis pequeñas.",
    ("ph", "HIGH"): "Agrega solución para bajar el pH (pH Down) en dosis pequeñas.",
    ("tds", "LOW"): "Agrega solución nutritiva A y B según la etapa del cultivo.",
    ("tds", "HIGH"): "Diluye la solución con agua limpia para bajar la concentración de sales.",
    ("soilMoisture", "LOW"): "Riega hasta humedecer el sustrato sin encharcarlo.",
    ("soilMoisture", "HIGH"): "Suspende el riego y revisa el drenaje para evitar la pudrición de raíces.",
}


@dataclass(frozen=True)
class ParameterDiagnosis:
    parameter: str
    value: float
    status: str
    severity: str
    deviation: float
    message: str
    recommendation: str | None


def is_rest_hour(hour: int | None) -> bool:
    return hour is not None and (hour >= REST_START_HOUR or hour < REST_END_HOUR)


def diagnose(profile: CropProfile, measures: dict[str, float | None],
             local_hour: int | None = None) -> list[ParameterDiagnosis]:
    resting = is_rest_hour(local_hour)
    results = []
    for parameter in PARAMETERS:
        value = measures.get(parameter)
        if value is None:
            continue
        rng = profile.ranges[parameter]
        deviation = rng.deviation(value)
        if resting and parameter == "brightness" and value < rng.min:
            status, severity, deviation = "REST", "OK", 0.0
            message = (f"Es de noche: la planta descansa y la luz baja ({_fmt(value)} {rng.unit}) es normal "
                       f"entre las {REST_START_HOUR}:00 y las {REST_END_HOUR}:00.")
        elif deviation == 0:
            status, severity = "OPTIMAL", "OK"
            message = f"{_capitalize(rng.label)} está en el rango ideal ({_fmt(rng.min)}–{_fmt(rng.max)} {rng.unit})."
        else:
            status = "LOW" if value < rng.min else "HIGH"
            severity = "CRITICAL" if deviation >= CRITICAL_DEVIATION else "WARNING"
            direction = "por debajo" if status == "LOW" else "por encima"
            message = (f"{_capitalize(rng.label)} ({_fmt(value)} {rng.unit}) está {direction} del rango ideal "
                       f"({_fmt(rng.min)}–{_fmt(rng.max)} {rng.unit}).")
        results.append(ParameterDiagnosis(parameter, value, status, severity, round(deviation, 3), message,
                                          RECOMMENDATIONS.get((parameter, status))))
    return results


def _capitalize(text: str) -> str:
    return text[:1].upper() + text[1:]


def _fmt(value: float) -> str:
    return f"{value:.1f}".rstrip("0").rstrip(".")

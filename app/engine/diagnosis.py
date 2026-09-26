"""Diagnóstico por variable: compara cada lectura con el perfil del cultivo."""

from dataclasses import dataclass

from app.knowledge.profiles import PARAMETERS, CropProfile

CRITICAL_DEVIATION = 1.0

RECOMMENDATIONS = {
    ("temperature", "LOW"): "Aleja la maceta de corrientes frías o usa una fuente de calor suave.",
    ("temperature", "HIGH"): "Ventila el espacio y evita la luz directa en las horas de más calor.",
    ("humidity", "LOW"): "Aumenta la humedad del ambiente con un humidificador o agrupando plantas.",
    ("humidity", "HIGH"): "Mejora la circulación de aire para prevenir hongos.",
    ("brightness", "LOW"): "Enciende la luz de cultivo o acerca la maceta a una fuente de luz.",
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


def diagnose(profile: CropProfile, measures: dict[str, float | None]) -> list[ParameterDiagnosis]:
    results = []
    for parameter in PARAMETERS:
        value = measures.get(parameter)
        if value is None:
            continue
        rng = profile.ranges[parameter]
        deviation = rng.deviation(value)
        if deviation == 0:
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

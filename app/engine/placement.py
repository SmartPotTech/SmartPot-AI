"""Consejo de lugar: se puede poner el cultivo donde se quiera, pero el asistente recomienda moverlo a su lugar ideal
y sube el tono a medida que el lugar elegido afecta su salud.

- OK: el lugar le sirve a la especie.
- UNKNOWN: no se sabe dónde está.
- TIP: no es su lugar ideal, pero hoy la planta está bien.
- MOVE: el lugar ya se nota en las lecturas (poca luz de día para una de sol; calor o exceso de luz para una de
  media sombra).
"""

from dataclasses import dataclass

from app.engine.diagnosis import ParameterDiagnosis
from app.knowledge.placement import LIGHT_NEEDS, SUN_LEVEL
from app.knowledge.profiles import CropProfile

PLACES = {
    ("OUTDOOR", "FULL_SUN"): "al aire libre a pleno sol",
    ("OUTDOOR", "PARTIAL_SUN"): "al aire libre en media sombra",
    ("OUTDOOR", "SHADE"): "al aire libre en sombra",
    ("INDOOR", "FULL_SUN"): "bajo techo junto a una ventana soleada",
    ("INDOOR", "PARTIAL_SUN"): "bajo techo con luz indirecta",
    ("INDOOR", "SHADE"): "bajo techo sin luz natural",
}


@dataclass(frozen=True)
class PlacementAdvice:
    level: str
    title: str
    message: str
    light_need: str
    ideal_setting: str
    ideal_exposure: str


def advise(profile: CropProfile, setting: str | None, exposure: str | None,
           diagnosis: list[ParameterDiagnosis], sunny_outside: bool = False) -> PlacementAdvice:
    need = LIGHT_NEEDS[profile.type]
    ideal_setting, ideal_exposure = need.ideal
    name = profile.name.lower()
    # Concordancia del pronombre: la lechuga, el tomate.
    o = "a" if name.endswith("a") else "o"

    def result(level: str, title: str, message: str) -> PlacementAdvice:
        return PlacementAdvice(level, title, message, need.level, ideal_setting, ideal_exposure)

    if setting is None or exposure is None:
        return result("UNKNOWN", f"¿Dónde está tu {name}?",
                      f"{need.note} Indica en Ajustes si está bajo techo o al aire libre y cuánto sol recibe, y el "
                      "asistente te dirá si el lugar le conviene.")

    sun = SUN_LEVEL[(setting, exposure)]
    place = PLACES[(setting, exposure)]
    if sun in need.comfortable:
        return result("OK", "El lugar le sienta bien", f"{need.note} Está {place}, justo lo que necesita.")

    status = {item.parameter: item.status for item in diagnosis}
    wants_more_sun = sun < min(need.comfortable)
    ideal_place = PLACES[need.ideal]
    if wants_more_sun:
        suffering = status.get("brightness") == "LOW"
        move = (f"muével{o} a pleno sol" if setting == "OUTDOOR"
                else f"sácal{o} al aire libre a pleno sol o, si no se puede, ponl{o} junto a la ventana más soleada")
        extra = " Afuera hay sol ahora mismo." if sunny_outside and suffering else ""
        if suffering:
            return result("MOVE", "Le falta sol",
                          f"Está {place} y la luz está baja: {move}.{extra} La luz ultravioleta ayuda, pero no "
                          f"reemplaza al sol. {need.note}")
        return result("TIP", "Estaría mejor con más sol",
                      f"Está {place}. {need.note} Hoy está bien; si ves tallos largos y pocas flores, {move}.")

    suffering = (status.get("temperature") == "HIGH" or status.get("brightness") == "HIGH"
                 or (status.get("humidity") == "LOW" and status.get("temperature") != "LOW"))
    if suffering:
        return result("MOVE", "Le sobra sol",
                      f"Está {place} y el calor o el exceso de luz ya se notan: muével{o} {ideal_place} o "
                      f"cúbrel{o} con una malla de sombra en las horas de más sol. {need.note}")
    return result("TIP", "Estaría mejor con menos sol",
                  f"Está {place}. {need.note} Hoy está bien; si ves hojas quemadas o que se espiga, pásal{o} "
                  f"{ideal_place}.")

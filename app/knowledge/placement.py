"""Luz que pide cada especie y cuánto sol recibe cada lugar.

Al aire libre, pleno sol es de 6 a 8 horas de sol directo, media sombra de 3 a 5 y sombra menos de 3. Bajo techo
cuenta la ventana: la más soleada equivale a media sombra de afuera.
"""

from dataclasses import dataclass

SETTINGS = ("INDOOR", "OUTDOOR")
EXPOSURES = ("FULL_SUN", "PARTIAL_SUN", "SHADE")

# Escala de sol: 3 pleno sol al aire libre … 0 un rincón sin luz natural.
SUN_LEVEL = {
    ("OUTDOOR", "FULL_SUN"): 3,
    ("OUTDOOR", "PARTIAL_SUN"): 2,
    ("OUTDOOR", "SHADE"): 1,
    ("INDOOR", "FULL_SUN"): 2,
    ("INDOOR", "PARTIAL_SUN"): 1,
    ("INDOOR", "SHADE"): 0,
}


@dataclass(frozen=True)
class LightNeed:
    level: str
    ideal: tuple[str, str]
    # Niveles de sol con los que la especie está cómoda.
    comfortable: frozenset[int]
    note: str


SUN_LOVER = frozenset({3})
HALF_SHADE = frozenset({1, 2})

LIGHT_NEEDS: dict[str, LightNeed] = {
    "LETTUCE": LightNeed("PARTIAL_SUN", ("OUTDOOR", "PARTIAL_SUN"), HALF_SHADE,
                         "La lechuga prefiere media sombra: con sol fuerte se calienta, se espiga y amarga."),
    "SPINACH": LightNeed("PARTIAL_SUN", ("OUTDOOR", "PARTIAL_SUN"), HALF_SHADE,
                         "La espinaca crece mejor en media sombra y con clima fresco."),
    "TOMATO": LightNeed("FULL_SUN", ("OUTDOOR", "FULL_SUN"), SUN_LOVER,
                        "El tomate es de sol: necesita de 6 a 8 horas de sol directo para florecer y dar fruto."),
    "PEPPER": LightNeed("FULL_SUN", ("OUTDOOR", "FULL_SUN"), SUN_LOVER,
                        "El pimentón es de sol: necesita de 6 a 8 horas de sol directo."),
    "STRAWBERRY": LightNeed("FULL_SUN", ("OUTDOOR", "FULL_SUN"), SUN_LOVER,
                            "La fresa necesita al menos 6 horas de sol directo para dar frutos dulces."),
    "BASIL": LightNeed("FULL_SUN", ("OUTDOOR", "FULL_SUN"), SUN_LOVER,
                       "La albahaca es de sol: con 6 horas de sol directo crece compacta y aromática."),
}

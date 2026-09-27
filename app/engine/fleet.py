"""Análisis de todos los cultivos de una cuenta a la vez.

- Ranking por índice de salud (lógica difusa de cada cultivo).
- Problemas compartidos: la misma variable fuera de rango en la mitad o más de los cultivos
  suele ser un problema del entorno (la habitación, el agua, la solución nutritiva) y no de un cultivo.
- Grupos por condiciones similares: K-Means sobre las variables normalizadas respecto a cada perfil,
  así una lechuga y un tomate son comparables. La cantidad de grupos se elige por silueta.
- Acciones agregadas: las que el agente propone para cada cultivo, reunidas por actuador para
  aplicarlas en bloque.
"""

from dataclasses import dataclass, field

import numpy as np
from sklearn.cluster import KMeans
from sklearn.metrics import silhouette_score

from app.engine import agent
from app.engine.diagnosis import ParameterDiagnosis, diagnose, is_rest_hour
from app.engine.fuzzy import HealthResult, health_index
from app.engine.models import normalize
from app.knowledge.profiles import get_profile

GROUP_PARAMETERS = ("temperature", "humidity", "ph", "tds", "soilMoisture")
SHARED_SHARE = 0.5
# Cómo se nombra cada variable fuera de rango (por debajo, por encima).
PHRASES = {
    "temperature": ("temperatura baja", "temperatura alta"),
    "humidity": ("aire seco", "aire muy húmedo"),
    "brightness": ("poca luz", "exceso de luz"),
    "ph": ("pH bajo", "pH alto"),
    "tds": ("pocos nutrientes", "exceso de nutrientes"),
    "soilMoisture": ("sustrato seco", "sustrato encharcado"),
}


def phrase(parameter: str, status: str) -> str:
    low, high = PHRASES[parameter]
    return low if status == "LOW" else high


@dataclass
class CropInput:
    id: str
    name: str
    crop_type: str
    measures: dict[str, float | None] | None
    actuators: set[str] = field(default_factory=set)


@dataclass
class CropResult:
    id: str
    name: str
    crop_type: str
    health: HealthResult | None
    diagnosis: list[ParameterDiagnosis]
    rank: int = 0


@dataclass(frozen=True)
class SharedIssue:
    parameter: str
    status: str
    crop_ids: list[str]
    share: float
    message: str


@dataclass(frozen=True)
class Group:
    label: str
    crop_ids: list[str]
    description: str


@dataclass(frozen=True)
class FleetAction:
    actuator: str
    action: str
    duration_seconds: int | None
    crop_ids: list[str]
    reason: str


@dataclass
class FleetAnalysis:
    average_health: float | None
    crops: list[CropResult]
    shared_issues: list[SharedIssue]
    groups: list[Group]
    actions: list[FleetAction]
    summary: str


def analyze_fleet(crops: list[CropInput], local_hour: int | None = None) -> FleetAnalysis:
    results = []
    for crop in crops:
        profile = get_profile(crop.crop_type)
        diagnosis = diagnose(profile, crop.measures, local_hour) if crop.measures else []
        health = health_index({d.parameter: d.deviation for d in diagnosis}) if diagnosis else None
        results.append(CropResult(crop.id, crop.name, profile.type, health, diagnosis))

    measured = sorted((r for r in results if r.health), key=lambda r: r.health.index)
    for rank, result in enumerate(measured, 1):
        result.rank = rank
    average = round(float(np.mean([r.health.index for r in measured])), 1) if measured else None

    shared = _shared_issues(measured)
    groups = _groups([c for c in crops if c.measures], measured)
    actions = _actions(crops, results, local_hour)
    return FleetAnalysis(average, results, shared, groups, actions, _summary(average, measured, shared, actions))


def _shared_issues(results: list[CropResult]) -> list[SharedIssue]:
    if len(results) < 2:
        return []
    issues = []
    for parameter in PHRASES:
        for status in ("LOW", "HIGH"):
            ids = [r.id for r in results if any(d.parameter == parameter and d.status == status for d in r.diagnosis)]
            share = len(ids) / len(results)
            if len(ids) >= 2 and share >= SHARED_SHARE:
                text = phrase(parameter, status)
                message = (f"{text[:1].upper() + text[1:]} en {len(ids)} de {len(results)} cultivos: "
                           "probablemente es el entorno y no un cultivo en particular.")
                issues.append(SharedIssue(parameter, status, ids, round(share, 2), message))
    return sorted(issues, key=lambda issue: issue.share, reverse=True)


def _groups(crops: list[CropInput], results: list[CropResult]) -> list[Group]:
    if len(crops) < 3:
        return []
    features, ids = [], []
    for crop in crops:
        positions = normalize(get_profile(crop.crop_type), crop.measures)
        features.append([positions.get(p, 0.5) for p in GROUP_PARAMETERS])
        ids.append(crop.id)
    data = np.clip(np.array(features, dtype=float), -2.0, 3.0)
    distinct = len(np.unique(np.round(data, 2), axis=0))
    if distinct < 2:
        return [Group("Condiciones similares", ids, "Todos los cultivos están en condiciones parecidas.")]
    best = None
    for k in range(2, min(4, distinct - 1, len(ids) - 1) + 1):
        model = KMeans(n_clusters=k, n_init=10, random_state=42).fit(data)
        score = silhouette_score(data, model.labels_)
        if best is None or score > best[0]:
            best = (score, model)
    model = best[1] if best else KMeans(n_clusters=2, n_init=10, random_state=42).fit(data)
    merged: dict[str, Group] = {}
    for cluster in range(model.n_clusters):
        members = [ids[i] for i in np.where(model.labels_ == cluster)[0]]
        label, description = _describe_group(model.cluster_centers_[cluster])
        previous = merged.get(label)
        merged[label] = Group(label, (previous.crop_ids if previous else []) + members, description)
    groups = list(merged.values())
    health = {r.id: r.health.index for r in results if r.health}
    return sorted(groups, key=lambda g: np.mean([health.get(i, 100) for i in g.crop_ids]))


def _describe_group(center: np.ndarray) -> tuple[str, str]:
    traits = []
    for value, parameter in zip(center, GROUP_PARAMETERS, strict=True):
        if value < -0.15:
            traits.append(phrase(parameter, "LOW"))
        elif value > 1.15:
            traits.append(phrase(parameter, "HIGH"))
    if not traits:
        return "En rango", "Las variables del grupo están, en promedio, dentro de su rango ideal."
    label = " y ".join(traits[:2])
    return label[:1].upper() + label[1:], "En promedio: " + ", ".join(traits) + "."


def _actions(crops: list[CropInput], results: list[CropResult], local_hour: int | None) -> list[FleetAction]:
    merged: dict[tuple[str, str], dict] = {}
    for crop, result in zip(crops, results, strict=True):
        if not result.diagnosis:
            continue
        for action in agent.decide(result.diagnosis, {}, {}, crop.actuators, resting=is_rest_hour(local_hour)):
            entry = merged.setdefault((action.actuator, action.action),
                                      {"duration": action.duration_seconds, "ids": [], "reason": action.reason})
            entry["ids"].append(crop.id)
            if action.duration_seconds and (entry["duration"] or 0) < action.duration_seconds:
                entry["duration"] = action.duration_seconds
    actions = [FleetAction(actuator, action, entry["duration"], entry["ids"], entry["reason"])
               for (actuator, action), entry in merged.items()]
    return sorted(actions, key=lambda a: len(a.crop_ids), reverse=True)


def _summary(average: float | None, results: list[CropResult], shared: list[SharedIssue],
             actions: list[FleetAction]) -> str:
    if average is None:
        return "Todavía no hay lecturas para comparar tus cultivos."
    text = f"Tus {len(results)} cultivos con lecturas promedian {average:.0f}/100 de salud."
    worst = results[0]
    if worst.health.index < 70:
        text += f" El que más atención necesita es «{worst.name}» ({worst.health.index:.0f}/100)."
    if shared:
        noun = "problema compartido" if len(shared) == 1 else "problemas compartidos"
        text += f" Hay {len(shared)} {noun} del entorno."
    if actions:
        text += f" El asistente sugiere {len(actions)} {'acción' if len(actions) == 1 else 'acciones'} en bloque."
    return text

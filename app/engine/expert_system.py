"""Motor de inferencia de encadenamiento hacia adelante.

Cada ciclo arma la agenda con las reglas cuyas condiciones se cumplen en la memoria de trabajo,
dispara la de mayor prioridad (refracción: cada regla se dispara una sola vez) y agrega sus
conclusiones como hechos nuevos, que pueden activar otras reglas. La traza de disparos es la
explicación que ve el usuario.
"""

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any

Condition = Callable[["WorkingMemory"], bool]
Conclusion = Callable[["WorkingMemory"], dict[str, Any]]
Message = Callable[["WorkingMemory"], str]


@dataclass
class WorkingMemory:
    facts: dict[str, Any] = field(default_factory=dict)

    def get(self, name: str, default: Any = None) -> Any:
        return self.facts.get(name, default)

    def has(self, name: str) -> bool:
        return name in self.facts

    def assert_fact(self, name: str, value: Any = True) -> None:
        self.facts[name] = value


@dataclass(frozen=True)
class Rule:
    name: str
    title: str
    condition: Condition
    message: Message
    certainty: float = 1.0
    salience: int = 0
    conclude: Conclusion = lambda memory: {}


@dataclass(frozen=True)
class Firing:
    rule: str
    title: str
    message: str
    certainty: float


class InferenceEngine:
    def __init__(self, rules: list[Rule], max_cycles: int = 50):
        self.rules = sorted(rules, key=lambda rule: rule.salience, reverse=True)
        self.max_cycles = max_cycles

    def run(self, memory: WorkingMemory) -> list[Firing]:
        fired: set[str] = set()
        trace: list[Firing] = []
        for _ in range(self.max_cycles):
            agenda = [rule for rule in self.rules if rule.name not in fired and rule.condition(memory)]
            if not agenda:
                break
            rule = agenda[0]
            fired.add(rule.name)
            for name, value in rule.conclude(memory).items():
                memory.assert_fact(name, value)
            memory.assert_fact(f"rule:{rule.name}", rule.certainty)
            trace.append(Firing(rule.name, rule.title, rule.message(memory), rule.certainty))
        return trace

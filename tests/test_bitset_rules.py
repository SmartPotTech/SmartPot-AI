from app.engine.bitset_rules import (
    BIT_H_LOW,
    BIT_T_HIGH,
    BitsetRuleEngine,
    encode_memory_to_bitset,
)
from app.engine.expert_system import WorkingMemory


def test_bitset_encoding_and_heat_stress():
    memory = WorkingMemory({
        "status:temperature": "HIGH",
        "status:humidity": "LOW",
        "value:temperature": 32.0,
    })

    bitset = encode_memory_to_bitset(memory)
    assert (bitset & BIT_T_HIGH) != 0
    assert (bitset & BIT_H_LOW) != 0

    engine = BitsetRuleEngine()
    conclusions = engine.evaluate(memory)
    names = [c.rule for c in conclusions]
    assert "heat_stress" in names


def test_bitset_encoding_ideal_conditions():
    memory = WorkingMemory({
        "status:temperature": "OPTIMAL",
        "status:humidity": "OPTIMAL",
        "status:ph": "OPTIMAL",
        "status:tds": "OPTIMAL",
        "status:soilMoisture": "OPTIMAL",
        "status:brightness": "OPTIMAL",
    })

    engine = BitsetRuleEngine()
    conclusions = engine.evaluate(memory)
    names = [c.rule for c in conclusions]
    assert "ideal_conditions" in names

import pytest

from app.knowledge.profiles import PARAMETERS, PROFILES, get_profile


def test_every_profile_covers_every_parameter_with_valid_ranges():
    assert set(PROFILES) == {"TOMATO", "LETTUCE", "STRAWBERRY", "BASIL", "SPINACH", "PEPPER"}
    for profile in PROFILES.values():
        assert set(profile.ranges) == set(PARAMETERS)
        for rng in profile.ranges.values():
            assert rng.min < rng.max
            assert rng.tolerance > 0


def test_deviation_is_zero_inside_and_grows_outside():
    temperature = get_profile("lettuce").ranges["temperature"]
    assert temperature.deviation(18) == 0
    assert temperature.deviation(27) == pytest.approx(1.0)
    assert temperature.deviation(12.5) == pytest.approx(0.5)


def test_unknown_crop_raises():
    with pytest.raises(KeyError):
        get_profile("MINT")

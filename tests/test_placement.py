from app.engine.diagnosis import diagnose
from app.engine.placement import advise
from app.knowledge.placement import LIGHT_NEEDS
from app.knowledge.profiles import PROFILES, get_profile


def advice(crop: str, setting, exposure, measures: dict | None = None, hour: int = 12, sunny: bool = False):
    profile = get_profile(crop)
    return advise(profile, setting, exposure, diagnose(profile, measures or {}, hour), sunny_outside=sunny)


def test_every_species_knows_the_light_it_needs():
    assert set(LIGHT_NEEDS) == set(PROFILES)


def test_without_a_place_the_assistant_asks_for_it():
    result = advice("TOMATO", None, None)
    assert result.level == "UNKNOWN"
    assert result.ideal_setting == "OUTDOOR" and result.ideal_exposure == "FULL_SUN"


def test_the_right_place_is_praised():
    assert advice("TOMATO", "OUTDOOR", "FULL_SUN").level == "OK"
    assert advice("LETTUCE", "OUTDOOR", "PARTIAL_SUN").level == "OK"
    assert advice("LETTUCE", "INDOOR", "FULL_SUN").level == "OK"


def test_a_wrong_place_is_allowed_but_the_tone_rises_with_the_symptoms():
    calm = advice("TOMATO", "INDOOR", "PARTIAL_SUN", {"brightness": 900})
    dark = advice("TOMATO", "INDOOR", "PARTIAL_SUN", {"brightness": 150}, sunny=True)
    assert calm.level == "TIP"
    assert dark.level == "MOVE"
    assert "sácalo" in dark.message and "ultravioleta" in dark.message and "Afuera hay sol" in dark.message


def test_shade_lovers_under_a_strong_sun_are_asked_to_move():
    calm = advice("LETTUCE", "OUTDOOR", "FULL_SUN", {"temperature": 20, "brightness": 1000})
    hot = advice("LETTUCE", "OUTDOOR", "FULL_SUN", {"temperature": 29, "brightness": 1900})
    assert calm.level == "TIP"
    assert hot.level == "MOVE"
    assert "muévela al aire libre en media sombra" in hot.message


def test_low_light_at_night_is_not_a_reason_to_move():
    assert advice("TOMATO", "OUTDOOR", "PARTIAL_SUN", {"brightness": 10}, hour=23).level == "TIP"

//! Consejo de lugar: se puede poner el cultivo donde se quiera, pero el asistente recomienda moverlo a su lugar ideal
//! y sube el tono a medida que el lugar elegido afecta su salud.
//!
//! - OK: el lugar le sirve a la especie.
//! - UNKNOWN: no se sabe dónde está.
//! - TIP: no es su lugar ideal, pero hoy la planta está bien.
//! - MOVE: el lugar ya se nota en las lecturas (poca luz de día para una de sol; calor o exceso de luz para una de
//!   media sombra).

use crate::domain::placement::{light_need, place_text, sun_level, Exposure, Setting};
use crate::domain::{CropProfile, Parameter};
use crate::engine::diagnosis::{ParameterDiagnosis, Status};
use crate::schema::response::PlacementAdvice;

pub fn advise(
    profile: &CropProfile,
    setting: Option<Setting>,
    exposure: Option<Exposure>,
    diagnosis: &[ParameterDiagnosis],
    sunny_outside: bool,
) -> PlacementAdvice {
    let need = light_need(profile.kind);
    let (ideal_setting, ideal_exposure) = need.ideal;
    let name = profile.name.to_lowercase();
    let result = |level: &'static str, title: String, message: String| PlacementAdvice {
        level,
        title,
        message,
        light_need: need.level,
        ideal_setting: ideal_setting.name(),
        ideal_exposure: ideal_exposure.name(),
    };

    let (Some(setting), Some(exposure)) = (setting, exposure) else {
        return result(
            "UNKNOWN",
            format!("Where is your {name}?"),
            format!(
                "{} Set whether it is indoors or outdoors and how much sun it receives in Settings, and the assistant will tell you if the spot suits it.",
                need.note
            ),
        );
    };

    let sun = sun_level(setting, exposure);
    let place = place_text(setting, exposure);
    if need.comfortable.contains(&sun) {
        return result(
            "OK",
            "Good location".into(),
            format!("{} It is {place}, just what it needs.", need.note),
        );
    }

    let status = |p: Parameter| {
        diagnosis
            .iter()
            .find(|d| d.parameter == p)
            .map(|d| d.status)
    };
    let wants_more_sun = sun < need.comfortable.iter().copied().min().unwrap_or(0);
    let ideal_place = place_text(ideal_setting, ideal_exposure);
    if wants_more_sun {
        let suffering = status(Parameter::Brightness) == Some(Status::Low);
        let movement = if setting == Setting::Outdoor {
            "move it to full sun"
        } else {
            "move it outdoors to full sun or, if not possible, place it next to the sunniest window"
        };
        let extra = if sunny_outside && suffering {
            " It is sunny outside right now."
        } else {
            ""
        };
        if suffering {
            return result(
                "MOVE",
                "Needs more sun".into(),
                format!(
                    "It is {place} and light is low: {movement}.{extra} UV light helps, but it does not replace sunlight. {}",
                    need.note
                ),
            );
        }
        return result(
            "TIP",
            "Would do better with more sun".into(),
            format!(
                "It is {place}. {} It is fine today; if you see leggy stems and few flowers, {movement}.",
                need.note
            ),
        );
    }

    let temperature = status(Parameter::Temperature);
    let suffering = temperature == Some(Status::High)
        || status(Parameter::Brightness) == Some(Status::High)
        || (status(Parameter::Humidity) == Some(Status::Low) && temperature != Some(Status::Low));
    if suffering {
        return result(
            "MOVE",
            "Too much sun".into(),
            format!(
                "It is {place} and heat or excess light is already noticeable: move it {ideal_place} or cover it with shade cloth during peak sun hours. {}",
                need.note
            ),
        );
    }
    result(
        "TIP",
        "Would do better with less sun".into(),
        format!(
            "It is {place}. {} It is fine today; if you see scorched leaves or bolting, move it {ideal_place}.",
            need.note
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile;

    #[test]
    fn approves_partial_shade_for_lettuce() {
        let lettuce = profile("LETTUCE").unwrap();
        let advice = advise(
            lettuce,
            Some(Setting::Outdoor),
            Some(Exposure::PartialSun),
            &[],
            false,
        );
        assert_eq!((advice.level, advice.ideal_exposure), ("OK", "PARTIAL_SUN"));
    }

    #[test]
    fn asks_where_the_crop_is() {
        let tomato = profile("TOMATO").unwrap();
        let advice = advise(tomato, Some(Setting::Indoor), None, &[], false);
        assert_eq!(advice.level, "UNKNOWN");
        assert_eq!(advice.title, "Where is your tomato?");
    }

    #[test]
    fn suggests_relocation_direction() {
        let pepper = profile("PEPPER").unwrap();
        let advice = advise(
            pepper,
            Some(Setting::Outdoor),
            Some(Exposure::Shade),
            &[],
            false,
        );
        assert!(
            advice.message.contains("move it to full sun"),
            "{}",
            advice.message
        );
        let lettuce = profile("LETTUCE").unwrap();
        let advice = advise(
            lettuce,
            Some(Setting::Outdoor),
            Some(Exposure::FullSun),
            &[],
            false,
        );
        assert!(
            advice.message.contains("move it outdoors in partial shade"),
            "{}",
            advice.message
        );
    }
}

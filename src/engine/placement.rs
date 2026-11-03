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
    // Concordancia del pronombre: la lechuga, el tomate.
    let o = if name.ends_with('a') { "a" } else { "o" };
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
            format!("¿Dónde está tu {name}?"),
            format!(
                "{} Indica en Ajustes si está bajo techo o al aire libre y cuánto sol recibe, y el asistente te dirá si el lugar le conviene.",
                need.note
            ),
        );
    };

    let sun = sun_level(setting, exposure);
    let place = place_text(setting, exposure);
    if need.comfortable.contains(&sun) {
        return result(
            "OK",
            "El lugar le sienta bien".into(),
            format!("{} Está {place}, justo lo que necesita.", need.note),
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
            format!("muével{o} a pleno sol")
        } else {
            format!("sácal{o} al aire libre a pleno sol o, si no se puede, ponl{o} junto a la ventana más soleada")
        };
        let extra = if sunny_outside && suffering {
            " Afuera hay sol ahora mismo."
        } else {
            ""
        };
        if suffering {
            return result(
                "MOVE",
                "Le falta sol".into(),
                format!(
                    "Está {place} y la luz está baja: {movement}.{extra} La luz ultravioleta ayuda, pero no reemplaza al sol. {}",
                    need.note
                ),
            );
        }
        return result(
            "TIP",
            "Estaría mejor con más sol".into(),
            format!(
                "Está {place}. {} Hoy está bien; si ves tallos largos y pocas flores, {movement}.",
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
            "Le sobra sol".into(),
            format!(
                "Está {place} y el calor o el exceso de luz ya se notan: muével{o} {ideal_place} o cúbrel{o} con una malla de sombra en las horas de más sol. {}",
                need.note
            ),
        );
    }
    result(
        "TIP",
        "Estaría mejor con menos sol".into(),
        format!(
            "Está {place}. {} Hoy está bien; si ves hojas quemadas o que se espiga, pásal{o} {ideal_place}.",
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
        assert_eq!(advice.title, "¿Dónde está tu tomate?");
    }

    #[test]
    fn uses_the_right_pronoun() {
        let pepper = profile("PEPPER").unwrap();
        let advice = advise(
            pepper,
            Some(Setting::Outdoor),
            Some(Exposure::Shade),
            &[],
            false,
        );
        assert!(
            advice.message.contains("muévelo a pleno sol"),
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
            advice
                .message
                .contains("pásala al aire libre en media sombra"),
            "{}",
            advice.message
        );
    }
}

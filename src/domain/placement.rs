//! Luz que pide cada especie y cuánto sol recibe cada lugar.
//!
//! Al aire libre, pleno sol es de 6 a 8 horas de sol directo, media sombra de 3 a 5 y sombra menos de 3. Bajo techo
//! cuenta la ventana: la más soleada equivale a media sombra de afuera.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Indoor,
    Outdoor,
}

impl Setting {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "INDOOR" => Some(Setting::Indoor),
            "OUTDOOR" => Some(Setting::Outdoor),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Setting::Indoor => "INDOOR",
            Setting::Outdoor => "OUTDOOR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exposure {
    FullSun,
    PartialSun,
    Shade,
}

impl Exposure {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "FULL_SUN" => Some(Exposure::FullSun),
            "PARTIAL_SUN" => Some(Exposure::PartialSun),
            "SHADE" => Some(Exposure::Shade),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Exposure::FullSun => "FULL_SUN",
            Exposure::PartialSun => "PARTIAL_SUN",
            Exposure::Shade => "SHADE",
        }
    }
}

/// Escala de sol: 3 pleno sol al aire libre … 0 un rincón sin luz natural.
pub fn sun_level(setting: Setting, exposure: Exposure) -> u8 {
    match (setting, exposure) {
        (Setting::Outdoor, Exposure::FullSun) => 3,
        (Setting::Outdoor, Exposure::PartialSun) => 2,
        (Setting::Outdoor, Exposure::Shade) => 1,
        (Setting::Indoor, Exposure::FullSun) => 2,
        (Setting::Indoor, Exposure::PartialSun) => 1,
        (Setting::Indoor, Exposure::Shade) => 0,
    }
}

/// Cómo se nombra cada lugar en los consejos.
pub fn place_text(setting: Setting, exposure: Exposure) -> &'static str {
    match (setting, exposure) {
        (Setting::Outdoor, Exposure::FullSun) => "outdoors in full sun",
        (Setting::Outdoor, Exposure::PartialSun) => "outdoors in partial shade",
        (Setting::Outdoor, Exposure::Shade) => "outdoors in shade",
        (Setting::Indoor, Exposure::FullSun) => "indoors next to a sunny window",
        (Setting::Indoor, Exposure::PartialSun) => "indoors with indirect light",
        (Setting::Indoor, Exposure::Shade) => "indoors without natural light",
    }
}

/// Luz que pide una especie.
#[derive(Debug, Clone, Copy)]
pub struct LightNeed {
    pub level: &'static str,
    pub ideal: (Setting, Exposure),
    /// Niveles de sol con los que la especie está cómoda.
    pub comfortable: &'static [u8],
    pub note: &'static str,
}

const SUN_LOVER: &[u8] = &[3];
const HALF_SHADE: &[u8] = &[1, 2];

pub fn light_need(crop_type: &str) -> LightNeed {
    let partial = (Setting::Outdoor, Exposure::PartialSun);
    let full = (Setting::Outdoor, Exposure::FullSun);
    let (level, ideal, comfortable, note) = match crop_type {
        "LETTUCE" => (
            "PARTIAL_SUN",
            partial,
            HALF_SHADE,
            "Lettuce prefers partial shade: under strong sun it overheats, bolts, and turns bitter.",
        ),
        "SPINACH" => (
            "PARTIAL_SUN",
            partial,
            HALF_SHADE,
            "Spinach grows best in partial shade and cool weather.",
        ),
        "TOMATO" => (
            "FULL_SUN",
            full,
            SUN_LOVER,
            "Tomatoes are sun-loving: they need 6 to 8 hours of direct sunlight to flower and fruit.",
        ),
        "PEPPER" => (
            "FULL_SUN",
            full,
            SUN_LOVER,
            "Bell peppers are sun-loving: they need 6 to 8 hours of direct sunlight.",
        ),
        "STRAWBERRY" => (
            "FULL_SUN",
            full,
            SUN_LOVER,
            "Strawberries need at least 6 hours of direct sunlight to produce sweet fruit.",
        ),
        _ => (
            "FULL_SUN",
            full,
            SUN_LOVER,
            "Basil is sun-loving: with 6 hours of direct sunlight it grows compact and aromatic.",
        ),
    };
    LightNeed {
        level,
        ideal,
        comfortable,
        note,
    }
}

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
        (Setting::Outdoor, Exposure::FullSun) => "al aire libre a pleno sol",
        (Setting::Outdoor, Exposure::PartialSun) => "al aire libre en media sombra",
        (Setting::Outdoor, Exposure::Shade) => "al aire libre en sombra",
        (Setting::Indoor, Exposure::FullSun) => "bajo techo junto a una ventana soleada",
        (Setting::Indoor, Exposure::PartialSun) => "bajo techo con luz indirecta",
        (Setting::Indoor, Exposure::Shade) => "bajo techo sin luz natural",
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
            "La lechuga prefiere media sombra: con sol fuerte se calienta, se espiga y amarga.",
        ),
        "SPINACH" => ("PARTIAL_SUN", partial, HALF_SHADE, "La espinaca crece mejor en media sombra y con clima fresco."),
        "TOMATO" => (
            "FULL_SUN",
            full,
            SUN_LOVER,
            "El tomate es de sol: necesita de 6 a 8 horas de sol directo para florecer y dar fruto.",
        ),
        "PEPPER" => ("FULL_SUN", full, SUN_LOVER, "El pimentón es de sol: necesita de 6 a 8 horas de sol directo."),
        "STRAWBERRY" => (
            "FULL_SUN",
            full,
            SUN_LOVER,
            "La fresa necesita al menos 6 horas de sol directo para dar frutos dulces.",
        ),
        _ => (
            "FULL_SUN",
            full,
            SUN_LOVER,
            "La albahaca es de sol: con 6 horas de sol directo crece compacta y aromática.",
        ),
    };
    LightNeed {
        level,
        ideal,
        comfortable,
        note,
    }
}

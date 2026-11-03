//! Base de conocimiento: rangos óptimos por especie en la escala de los sensores del dispositivo.
//!
//! La conductividad se expresa en ppm con factor 700 (EC × 700) y la luz en lux relativos del sensor del
//! dispositivo (0 a 2000). La presión (`atmosphere`) se recibe y valida, pero no tiene rango por especie.

/// Variables con rango ideal, en el orden en que se diagnostican.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Parameter {
    Temperature,
    Humidity,
    Brightness,
    Ph,
    Tds,
    SoilMoisture,
}

impl Parameter {
    pub const ALL: [Parameter; 6] = [
        Parameter::Temperature,
        Parameter::Humidity,
        Parameter::Brightness,
        Parameter::Ph,
        Parameter::Tds,
        Parameter::SoilMoisture,
    ];

    /// Nombre del contrato (camelCase).
    pub fn name(self) -> &'static str {
        match self {
            Parameter::Temperature => "temperature",
            Parameter::Humidity => "humidity",
            Parameter::Brightness => "brightness",
            Parameter::Ph => "ph",
            Parameter::Tds => "tds",
            Parameter::SoilMoisture => "soilMoisture",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Rango ideal de una variable.
#[derive(Debug, Clone, Copy)]
pub struct Range {
    pub min: f64,
    pub max: f64,
    pub tolerance: f64,
    pub unit: &'static str,
    pub label: &'static str,
}

impl Range {
    /// 0 dentro del rango; fuera, la distancia medida en tolerancias.
    pub fn deviation(&self, value: f64) -> f64 {
        if value < self.min {
            (self.min - value) / self.tolerance
        } else if value > self.max {
            (value - self.max) / self.tolerance
        } else {
            0.0
        }
    }

    pub fn width(&self) -> f64 {
        self.max - self.min
    }
}

/// Perfil agronómico de una especie.
#[derive(Debug, Clone)]
pub struct CropProfile {
    pub kind: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub ranges: [Range; 6],
}

impl CropProfile {
    pub fn range(&self, parameter: Parameter) -> &Range {
        &self.ranges[parameter.index()]
    }
}

const fn ranges(
    temperature: (f64, f64),
    humidity: (f64, f64),
    brightness: (f64, f64),
    ph: (f64, f64),
    tds: (f64, f64),
    soil: (f64, f64),
) -> [Range; 6] {
    [
        Range {
            min: temperature.0,
            max: temperature.1,
            tolerance: 5.0,
            unit: "°C",
            label: "la temperatura",
        },
        Range {
            min: humidity.0,
            max: humidity.1,
            tolerance: 15.0,
            unit: "%",
            label: "la humedad del aire",
        },
        Range {
            min: brightness.0,
            max: brightness.1,
            tolerance: 300.0,
            unit: "lux",
            label: "la luz",
        },
        Range {
            min: ph.0,
            max: ph.1,
            tolerance: 0.8,
            unit: "pH",
            label: "el pH",
        },
        Range {
            min: tds.0,
            max: tds.1,
            tolerance: 400.0,
            unit: "ppm",
            label: "el nivel de nutrientes (TDS)",
        },
        Range {
            min: soil.0,
            max: soil.1,
            tolerance: 15.0,
            unit: "%",
            label: "la humedad del sustrato",
        },
    ]
}

/// Especies conocidas, en el orden del catálogo.
pub static PROFILES: [CropProfile; 6] = [
    CropProfile {
        kind: "LETTUCE",
        name: "Lechuga",
        description: "Hoja de clima fresco; se espiga con calor y luz excesiva.",
        ranges: ranges(
            (15.0, 22.0),
            (50.0, 70.0),
            (300.0, 1400.0),
            (5.5, 6.5),
            (560.0, 840.0),
            (60.0, 80.0),
        ),
    },
    CropProfile {
        kind: "TOMATO",
        name: "Tomate",
        description: "Fruto exigente en luz, calor y nutrientes durante la floración.",
        ranges: ranges(
            (20.0, 28.0),
            (60.0, 80.0),
            (600.0, 1800.0),
            (5.5, 6.5),
            (1400.0, 2800.0),
            (55.0, 75.0),
        ),
    },
    CropProfile {
        kind: "STRAWBERRY",
        name: "Fresa",
        description: "Sensible al exceso de sales y a la humedad alta, que favorece hongos.",
        ranges: ranges(
            (18.0, 26.0),
            (60.0, 75.0),
            (500.0, 1600.0),
            (5.5, 6.2),
            (700.0, 980.0),
            (60.0, 80.0),
        ),
    },
    CropProfile {
        kind: "BASIL",
        name: "Albahaca",
        description: "Aromática de clima cálido; tolera ambientes secos.",
        ranges: ranges(
            (20.0, 30.0),
            (40.0, 60.0),
            (500.0, 1600.0),
            (5.5, 6.5),
            (700.0, 1120.0),
            (50.0, 70.0),
        ),
    },
    CropProfile {
        kind: "SPINACH",
        name: "Espinaca",
        description: "Hoja de clima fresco con alta demanda de nitrógeno.",
        ranges: ranges(
            (15.0, 24.0),
            (50.0, 70.0),
            (300.0, 1400.0),
            (6.0, 7.0),
            (1260.0, 1610.0),
            (60.0, 80.0),
        ),
    },
    CropProfile {
        kind: "PEPPER",
        name: "Pimentón",
        description: "Fruto de clima cálido; necesita luz intensa y pH estable.",
        ranges: ranges(
            (21.0, 29.0),
            (50.0, 70.0),
            (600.0, 1800.0),
            (5.8, 6.5),
            (1400.0, 2100.0),
            (55.0, 75.0),
        ),
    },
];

/// Perfil de una especie, sin distinguir mayúsculas.
pub fn profile(crop_type: &str) -> Option<&'static CropProfile> {
    let wanted = crop_type.to_uppercase();
    PROFILES.iter().find(|p| p.kind == wanted)
}

/// Valores de las seis variables con rango, en el orden de [`Parameter::ALL`].
pub type Values = [Option<f64>; 6];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_valid_ranges() {
        for profile in &PROFILES {
            for range in &profile.ranges {
                assert!(range.min < range.max, "{} {}", profile.kind, range.label);
                assert!(range.tolerance > 0.0);
            }
        }
    }

    #[test]
    fn profiles_are_case_insensitive() {
        assert_eq!(profile("lettuce").map(|p| p.name), Some("Lechuga"));
        assert_eq!(profile("Pepper").map(|p| p.name), Some("Pimentón"));
        assert!(profile("CACTUS").is_none());
    }

    #[test]
    fn deviation_counts_tolerances() {
        let range = profile("LETTUCE").unwrap().range(Parameter::Temperature);
        assert_eq!(range.deviation(18.0), 0.0);
        assert_eq!(range.deviation(10.0), 1.0);
        assert_eq!(range.deviation(24.5), 0.5);
    }
}

//! Peticiones del contrato y su validación.
//!
//! Replica las reglas del servicio en Python: nombres en camelCase o snake_case, rangos físicos de los sensores,
//! especies conocidas (sin distinguir mayúsculas), listas con tope y fechas ISO 8601 con o sin zona (sin zona se
//! toman como UTC). Junta todos los errores con la ruta del campo, como `measures.temperature` o `history.3.ph`.

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::domain::placement::{Exposure, Setting};
use crate::domain::{profile, CropProfile, Parameter, Values};

/// Error de un campo, con la ruta del contrato.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

/// Marca de tiempo en microsegundos UTC (la resolución de `datetime` en Python).
pub type Micros = i64;

/// Lectura de sensores validada.
#[derive(Debug, Clone, Default)]
pub struct Measures {
    pub values: Values,
    pub atmosphere: Option<f64>,
    pub measured_at: Option<Micros>,
}

impl Measures {
    pub fn get(&self, parameter: Parameter) -> Option<f64> {
        self.values[parameter.index()]
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Place {
    pub setting: Option<Setting>,
    pub exposure: Option<Exposure>,
}

/// Clima actual del lugar del cultivo.
#[derive(Debug, Clone)]
pub struct Outside {
    pub temperature: f64,
    pub humidity: f64,
    pub precipitation: f64,
    pub radiation: f64,
    pub is_day: bool,
    pub condition: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InsightRequest {
    pub profile: &'static CropProfile,
    pub measures: Measures,
    pub history: Vec<Measures>,
    pub actuators: Vec<String>,
    pub local_hour: Option<u8>,
    pub placement: Option<Place>,
    pub weather: Option<Outside>,
}

#[derive(Debug, Clone)]
pub struct FleetCrop {
    pub id: String,
    pub name: String,
    pub profile: &'static CropProfile,
    pub measures: Option<Measures>,
    pub actuators: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FleetRequest {
    pub crops: Vec<FleetCrop>,
    pub local_hour: Option<u8>,
}

/// Límites físicos de cada sensor (los mismos del servicio en Python).
const LIMITS: [(&str, &str, f64, f64); 7] = [
    ("temperature", "temperature", -20.0, 60.0),
    ("humidity", "humidity", 0.0, 100.0),
    ("brightness", "brightness", 0.0, 200_000.0),
    ("ph", "ph", 0.0, 14.0),
    ("tds", "tds", 0.0, 10_000.0),
    ("atmosphere", "atmosphere", 300.0, 1_100.0),
    ("soilMoisture", "soil_moisture", 0.0, 100.0),
];

struct Validator {
    errors: Vec<FieldError>,
}

fn join(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}.{name}")
    }
}

fn field<'a>(object: &'a Map<String, Value>, camel: &str, snake: &str) -> Option<&'a Value> {
    object
        .get(camel)
        .or_else(|| object.get(snake))
        .filter(|v| !v.is_null())
}

impl Validator {
    fn new() -> Self {
        Self { errors: Vec::new() }
    }

    fn fail(&mut self, field: String, message: impl Into<String>) {
        self.errors.push(FieldError {
            field,
            message: message.into(),
        });
    }

    fn object<'a>(&mut self, value: &'a Value, path: &str) -> Option<&'a Map<String, Value>> {
        match value {
            Value::Object(map) => Some(map),
            _ => {
                self.fail(path.to_string(), "Debe ser un objeto");
                None
            }
        }
    }

    fn number(&mut self, value: &Value, path: &str) -> Option<f64> {
        let parsed = match value {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse::<f64>().ok(),
            _ => None,
        };
        match parsed {
            Some(v) if v.is_finite() => Some(v),
            _ => {
                self.fail(path.to_string(), "Debe ser un número");
                None
            }
        }
    }

    fn bounded(&mut self, value: &Value, path: &str, min: f64, max: f64) -> Option<f64> {
        let number = self.number(value, path)?;
        if number < min {
            self.fail(
                path.to_string(),
                format!("Debe ser mayor o igual que {}", plain(min)),
            );
            None
        } else if number > max {
            self.fail(
                path.to_string(),
                format!("Debe ser menor o igual que {}", plain(max)),
            );
            None
        } else {
            Some(number)
        }
    }

    fn integer(&mut self, value: &Value, path: &str, min: i64, max: i64) -> Option<i64> {
        let parsed = match value {
            Value::Number(n) => n.as_i64().or_else(|| {
                n.as_f64()
                    .filter(|f| f.fract() == 0.0 && f.abs() < 1e15)
                    .map(|f| f as i64)
            }),
            Value::String(s) => s.trim().parse::<i64>().ok(),
            _ => None,
        };
        let Some(number) = parsed else {
            self.fail(path.to_string(), "Debe ser un número entero");
            return None;
        };
        if number < min {
            self.fail(
                path.to_string(),
                format!("Debe ser mayor o igual que {min}"),
            );
            None
        } else if number > max {
            self.fail(
                path.to_string(),
                format!("Debe ser menor o igual que {max}"),
            );
            None
        } else {
            Some(number)
        }
    }

    fn text<'a>(&mut self, value: &'a Value, path: &str) -> Option<&'a str> {
        match value {
            Value::String(s) => Some(s),
            _ => {
                self.fail(path.to_string(), "Debe ser un texto");
                None
            }
        }
    }

    fn sized_text(&mut self, value: &Value, path: &str, min: usize, max: usize) -> Option<String> {
        let text = self.text(value, path)?;
        let length = text.chars().count();
        if length < min {
            self.fail(
                path.to_string(),
                format!("Debe tener al menos {min} caracteres"),
            );
            None
        } else if length > max {
            self.fail(
                path.to_string(),
                format!("Debe tener como máximo {max} caracteres"),
            );
            None
        } else {
            Some(text.to_string())
        }
    }

    fn boolean(&mut self, value: &Value, path: &str) -> Option<bool> {
        let parsed = match value {
            Value::Bool(b) => Some(*b),
            Value::Number(n) => match n.as_f64() {
                Some(1.0) => Some(true),
                Some(0.0) => Some(false),
                _ => None,
            },
            Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" | "t" | "y" => Some(true),
                "false" | "0" | "no" | "off" | "f" | "n" => Some(false),
                _ => None,
            },
            _ => None,
        };
        if parsed.is_none() {
            self.fail(path.to_string(), "Debe ser verdadero o falso");
        }
        parsed
    }

    fn list<'a>(
        &mut self,
        value: &'a Value,
        path: &str,
        min: usize,
        max: usize,
    ) -> Option<&'a Vec<Value>> {
        let Value::Array(items) = value else {
            self.fail(path.to_string(), "Debe ser una lista");
            return None;
        };
        if items.len() < min {
            self.fail(
                path.to_string(),
                format!("Debe tener al menos {min} elemento(s)"),
            );
            None
        } else if items.len() > max {
            self.fail(
                path.to_string(),
                format!("Debe tener como máximo {max} elementos"),
            );
            None
        } else {
            Some(items)
        }
    }

    fn crop_type(&mut self, value: Option<&Value>, path: &str) -> Option<&'static CropProfile> {
        let Some(value) = value else {
            self.fail(path.to_string(), "Campo obligatorio");
            return None;
        };
        let text = self.text(value, path)?;
        match profile(text) {
            Some(found) => Some(found),
            None => {
                self.fail(
                    path.to_string(),
                    format!("Tipo de cultivo desconocido: {text}"),
                );
                None
            }
        }
    }

    fn timestamp(&mut self, value: &Value, path: &str) -> Option<Micros> {
        let parsed = match value {
            Value::String(s) => parse_datetime(s.trim()),
            Value::Number(n) => n.as_f64().filter(|f| f.is_finite()).map(|f| {
                // Como pydantic: un número grande son milisegundos y uno chico, segundos.
                let seconds = if f.abs() > 2e10 { f / 1000.0 } else { f };
                (seconds * 1e6).round() as i64
            }),
            _ => None,
        };
        if parsed.is_none() {
            self.fail(path.to_string(), "Debe ser una fecha y hora ISO 8601");
        }
        parsed
    }

    fn measures(&mut self, value: &Value, path: &str) -> Option<Measures> {
        let object = self.object(value, path)?;
        let before = self.errors.len();
        let mut measures = Measures::default();
        for (camel, snake, min, max) in LIMITS {
            let Some(raw) = field(object, camel, snake) else {
                continue;
            };
            let number = self.bounded(raw, &join(path, camel), min, max);
            match camel {
                "atmosphere" => measures.atmosphere = number,
                "temperature" => measures.values[Parameter::Temperature.index()] = number,
                "humidity" => measures.values[Parameter::Humidity.index()] = number,
                "brightness" => measures.values[Parameter::Brightness.index()] = number,
                "ph" => measures.values[Parameter::Ph.index()] = number,
                "tds" => measures.values[Parameter::Tds.index()] = number,
                _ => measures.values[Parameter::SoilMoisture.index()] = number,
            }
        }
        if let Some(raw) = field(object, "measuredAt", "measured_at") {
            measures.measured_at = self.timestamp(raw, &join(path, "measuredAt"));
        }
        (self.errors.len() == before).then_some(measures)
    }

    fn actuators(&mut self, object: &Map<String, Value>, path: &str) -> Vec<String> {
        let Some(raw) = field(object, "actuators", "actuators") else {
            return Vec::new();
        };
        let target = join(path, "actuators");
        let Some(items) = self.list(raw, &target, 0, 20) else {
            return Vec::new();
        };
        let mut names = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            if let Some(text) = self.text(item, &join(&target, &i.to_string())) {
                names.push(text.to_string());
            }
        }
        names
    }

    fn local_hour(&mut self, object: &Map<String, Value>) -> Option<u8> {
        let raw = field(object, "localHour", "local_hour")?;
        self.integer(raw, "localHour", 0, 23).map(|h| h as u8)
    }

    fn place(&mut self, value: &Value) -> Option<Place> {
        let object = self.object(value, "placement")?;
        let mut place = Place::default();
        if let Some(raw) = field(object, "setting", "setting") {
            if let Some(text) = self.text(raw, "placement.setting") {
                place.setting = Setting::parse(text);
                if place.setting.is_none() {
                    self.fail(
                        "placement.setting".into(),
                        "Debe ser uno de: INDOOR, OUTDOOR",
                    );
                }
            }
        }
        if let Some(raw) = field(object, "exposure", "exposure") {
            if let Some(text) = self.text(raw, "placement.exposure") {
                place.exposure = Exposure::parse(text);
                if place.exposure.is_none() {
                    self.fail(
                        "placement.exposure".into(),
                        "Debe ser uno de: FULL_SUN, PARTIAL_SUN, SHADE",
                    );
                }
            }
        }
        Some(place)
    }

    fn outside(&mut self, value: &Value) -> Option<Outside> {
        let object = self.object(value, "weather")?;
        let required = |v: &mut Self, camel: &str, min: f64, max: f64| -> Option<f64> {
            match field(object, camel, camel) {
                Some(raw) => v.bounded(raw, &format!("weather.{camel}"), min, max),
                None => {
                    v.fail(format!("weather.{camel}"), "Campo obligatorio");
                    None
                }
            }
        };
        let temperature = required(self, "temperature", -60.0, 60.0);
        let humidity = required(self, "humidity", 0.0, 100.0);
        let precipitation = match field(object, "precipitation", "precipitation") {
            Some(raw) => self.bounded(raw, "weather.precipitation", 0.0, 500.0),
            None => Some(0.0),
        };
        let radiation = match field(object, "radiation", "radiation") {
            Some(raw) => self.bounded(raw, "weather.radiation", 0.0, 1500.0),
            None => Some(0.0),
        };
        let is_day = match field(object, "isDay", "is_day") {
            Some(raw) => self.boolean(raw, "weather.isDay"),
            None => Some(true),
        };
        let condition = match field(object, "condition", "condition") {
            Some(raw) => self.sized_text(raw, "weather.condition", 0, 20).map(Some),
            None => Some(None),
        };
        Some(Outside {
            temperature: temperature?,
            humidity: humidity?,
            precipitation: precipitation?,
            radiation: radiation?,
            is_day: is_day?,
            condition: condition?,
        })
    }
}

/// Número sin decimales sobrantes para los mensajes de error.
fn plain(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

/// Fecha ISO 8601 con o sin zona; sin zona se toma como UTC. Se trunca a microsegundos, como Python.
pub fn parse_datetime(text: &str) -> Option<Micros> {
    if let Ok(moment) = DateTime::parse_from_rfc3339(text) {
        return Some(moment.with_timezone(&Utc).timestamp_micros());
    }
    let normalized = text.replacen(' ', "T", 1);
    for format in [
        "%Y-%m-%dT%H:%M:%S%.f%:z",
        "%Y-%m-%dT%H:%M:%S%.f%z",
        "%Y-%m-%dT%H:%M%:z",
        "%Y-%m-%dT%H:%M%z",
    ] {
        if let Ok(moment) = DateTime::parse_from_str(&normalized, format) {
            return Some(moment.with_timezone(&Utc).timestamp_micros());
        }
    }
    let naive = normalized.trim_end_matches(['Z', 'z']);
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(moment) = NaiveDateTime::parse_from_str(naive, format) {
            return Some(moment.and_utc().timestamp_micros());
        }
    }
    NaiveDate::parse_from_str(naive, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|moment| moment.and_utc().timestamp_micros())
}

/// Valida el cuerpo de `POST /v1/insights`.
pub fn parse_insight(body: &Value) -> Result<InsightRequest, Vec<FieldError>> {
    let mut v = Validator::new();
    let Some(object) = v.object(body, "") else {
        return Err(v.errors);
    };

    let profile = v.crop_type(field(object, "cropType", "crop_type"), "cropType");
    let measures = match field(object, "measures", "measures") {
        Some(raw) => v.measures(raw, "measures"),
        None => {
            v.fail("measures".into(), "Campo obligatorio");
            None
        }
    };
    let mut history = Vec::new();
    if let Some(raw) = field(object, "history", "history") {
        if let Some(items) = v.list(raw, "history", 0, 500) {
            for (i, item) in items.iter().enumerate() {
                if let Some(point) = v.measures(item, &format!("history.{i}")) {
                    history.push(point);
                }
            }
        }
    }
    let actuators = v.actuators(object, "");
    let local_hour = v.local_hour(object);
    let placement = field(object, "placement", "placement").and_then(|raw| v.place(raw));
    let weather = field(object, "weather", "weather").and_then(|raw| v.outside(raw));

    if !v.errors.is_empty() {
        return Err(v.errors);
    }
    Ok(InsightRequest {
        profile: profile.expect("validado"),
        measures: measures.expect("validado"),
        history,
        actuators,
        local_hour,
        placement,
        weather,
    })
}

/// Valida el cuerpo de `POST /v1/fleet`.
pub fn parse_fleet(body: &Value) -> Result<FleetRequest, Vec<FieldError>> {
    let mut v = Validator::new();
    let Some(object) = v.object(body, "") else {
        return Err(v.errors);
    };

    let mut crops = Vec::new();
    match field(object, "crops", "crops") {
        Some(raw) => {
            if let Some(items) = v.list(raw, "crops", 1, 50) {
                for (i, item) in items.iter().enumerate() {
                    let path = format!("crops.{i}");
                    let Some(crop) = v.object(item, &path) else {
                        continue;
                    };
                    let before = v.errors.len();
                    let id = match field(crop, "id", "id") {
                        Some(raw) => v.sized_text(raw, &join(&path, "id"), 1, 64),
                        None => {
                            v.fail(join(&path, "id"), "Campo obligatorio");
                            None
                        }
                    };
                    let name = match field(crop, "name", "name") {
                        Some(raw) => v.sized_text(raw, &join(&path, "name"), 1, 80),
                        None => {
                            v.fail(join(&path, "name"), "Campo obligatorio");
                            None
                        }
                    };
                    let profile = v.crop_type(
                        field(crop, "cropType", "crop_type"),
                        &join(&path, "cropType"),
                    );
                    let measures = field(crop, "measures", "measures")
                        .and_then(|raw| v.measures(raw, &join(&path, "measures")));
                    let actuators = v.actuators(crop, &path);
                    if v.errors.len() == before {
                        crops.push(FleetCrop {
                            id: id.expect("validado"),
                            name: name.expect("validado"),
                            profile: profile.expect("validado"),
                            measures,
                            actuators,
                        });
                    }
                }
            }
        }
        None => v.fail("crops".into(), "Campo obligatorio"),
    }
    let local_hour = v.local_hour(object);
    if !v.errors.is_empty() {
        return Err(v.errors);
    }
    Ok(FleetRequest { crops, local_hour })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_datetimes_like_pydantic() {
        let utc = parse_datetime("2026-10-01T12:00:00Z").unwrap();
        assert_eq!(parse_datetime("2026-10-01T07:00:00-05:00"), Some(utc));
        assert_eq!(parse_datetime("2026-10-01T12:00:00"), Some(utc));
        assert_eq!(parse_datetime("2026-10-01 12:00:00"), Some(utc));
        assert_eq!(
            parse_datetime("2026-10-01T12:00:00.123456789Z"),
            Some(utc + 123_456)
        );
        assert!(parse_datetime("ayer").is_none());
    }

    #[test]
    fn accepts_snake_case_and_uppercases_the_crop() {
        let request =
            parse_insight(&json!({"crop_type": "basil", "measures": {"soil_moisture": 55}}))
                .unwrap();
        assert_eq!(request.profile.kind, "BASIL");
        assert_eq!(request.measures.get(Parameter::SoilMoisture), Some(55.0));
    }

    #[test]
    fn collects_errors_with_their_path() {
        let errors = parse_insight(&json!({
            "cropType": "CACTUS",
            "measures": {"temperature": 61},
            "history": [{"ph": 15}],
            "localHour": 24,
        }))
        .unwrap_err();
        let fields: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(
            fields,
            [
                "cropType",
                "measures.temperature",
                "history.0.ph",
                "localHour"
            ]
        );
        assert_eq!(errors[0].message, "Tipo de cultivo desconocido: CACTUS");
    }

    #[test]
    fn weather_needs_temperature_and_humidity() {
        let errors = parse_insight(
            &json!({"cropType": "BASIL", "measures": {}, "weather": {"temperature": 20}}),
        )
        .unwrap_err();
        assert_eq!(errors[0].field, "weather.humidity");
    }

    #[test]
    fn fleet_limits_the_number_of_crops() {
        assert!(parse_fleet(&json!({"crops": []})).is_err());
        let many: Vec<Value> = (0..51)
            .map(|i| json!({"id": i.to_string(), "name": "x", "cropType": "LETTUCE"}))
            .collect();
        assert!(parse_fleet(&json!({ "crops": many })).is_err());
    }
}

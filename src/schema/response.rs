//! Respuestas del contrato, con los mismos campos, nombres y orden que el servicio en Python.
//! Los campos opcionales se envían como `null`, igual que FastAPI.

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

/// Mapa que conserva el orden de inserción (los diccionarios de Python también lo hacen).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ordered<V>(pub Vec<(String, V)>);

impl<V: Serialize> Serialize for Ordered<V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub index: f64,
    pub level: &'static str,
    pub label: &'static str,
    pub by_parameter: Ordered<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    pub parameter: &'static str,
    pub value: f64,
    pub status: &'static str,
    pub severity: &'static str,
    pub message: String,
    pub recommendation: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conclusion {
    pub rule: &'static str,
    pub title: &'static str,
    pub message: String,
    pub certainty: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prediction {
    pub name: &'static str,
    pub label: &'static str,
    pub probability: f64,
    pub model: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Forecast {
    pub parameter: &'static str,
    pub current: f64,
    pub slope_per_hour: f64,
    #[serde(rename = "expectedIn3h")]
    pub expected_in_3h: f64,
    pub trend: &'static str,
    pub hours_to_limit: Option<f64>,
    pub limit: Option<&'static str>,
    pub confidence: f64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub actuator: &'static str,
    pub action: &'static str,
    pub duration_seconds: Option<i64>,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedPrediction {
    pub name: &'static str,
    pub label: String,
    pub probability: f64,
    pub model: String,
    pub metric: String,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedMoisture {
    #[serde(rename = "expectedIn1h")]
    pub expected_in_1h: f64,
    pub model: String,
    pub mae: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatingState {
    pub label: String,
    pub description: String,
    pub share: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedAnomaly {
    pub score: f64,
    pub unusual: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Learning {
    pub source: &'static str,
    pub readings: i64,
    pub trained_at: Option<String>,
    pub message: String,
    pub predictions: Vec<LearnedPrediction>,
    pub moisture: Option<LearnedMoisture>,
    pub state: Option<OperatingState>,
    pub anomaly: Option<LearnedAnomaly>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementAdvice {
    pub level: &'static str,
    pub title: String,
    pub message: String,
    pub light_need: &'static str,
    pub ideal_setting: &'static str,
    pub ideal_exposure: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightResponse {
    pub crop_type: &'static str,
    pub health: Health,
    pub diagnosis: Vec<Diagnosis>,
    pub conclusions: Vec<Conclusion>,
    pub predictions: Vec<Prediction>,
    pub actions: Vec<Action>,
    pub forecasts: Vec<Forecast>,
    pub learning: Option<Learning>,
    pub placement: Option<PlacementAdvice>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetCropResult {
    pub id: String,
    pub name: String,
    pub crop_type: &'static str,
    pub rank: Option<usize>,
    pub health: Option<Health>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedIssue {
    pub parameter: &'static str,
    pub status: &'static str,
    pub crop_ids: Vec<String>,
    pub share: f64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CropGroup {
    pub label: String,
    pub crop_ids: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetAction {
    pub actuator: &'static str,
    pub action: &'static str,
    pub duration_seconds: Option<i64>,
    pub crop_ids: Vec<String>,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetResponse {
    pub average_health: Option<f64>,
    pub crops: Vec<FleetCropResult>,
    pub shared_issues: Vec<SharedIssue>,
    pub groups: Vec<CropGroup>,
    pub actions: Vec<FleetAction>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RangeOut {
    pub min: f64,
    pub max: f64,
    pub unit: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct CropProfileOut {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub ranges: Ordered<RangeOut>,
}

impl CropProfileOut {
    pub fn from_profile(profile: &'static crate::domain::CropProfile) -> Self {
        let ranges = crate::domain::Parameter::ALL
            .iter()
            .map(|&p| {
                let r = profile.range(p);
                (
                    p.name().to_string(),
                    RangeOut {
                        min: r.min,
                        max: r.max,
                        unit: r.unit,
                        label: r.label,
                    },
                )
            })
            .collect();
        Self {
            kind: profile.kind,
            name: profile.name,
            description: profile.description,
            ranges: Ordered(ranges),
        }
    }
}

/// Fecha UTC como la serializa pydantic: `datetime.fromtimestamp(t, UTC)` con «Z» y microsegundos si los hay.
pub fn iso_utc(epoch_seconds: f64) -> String {
    let whole = epoch_seconds.trunc();
    let mut seconds = whole as i64;
    let mut micros = ((epoch_seconds - whole) * 1e6).round_ties_even() as i64;
    if micros >= 1_000_000 {
        seconds += 1;
        micros -= 1_000_000;
    } else if micros < 0 {
        seconds -= 1;
        micros += 1_000_000;
    }
    let Some(moment) = chrono::DateTime::from_timestamp(seconds, (micros * 1000) as u32) else {
        return String::new();
    };
    if micros == 0 {
        moment.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    } else {
        moment.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_utc_like_pydantic() {
        assert_eq!(iso_utc(1_790_000_000.0), "2026-09-21T14:13:20Z");
        assert_eq!(iso_utc(1_790_000_000.25), "2026-09-21T14:13:20.250000Z");
    }

    #[test]
    fn ordered_map_keeps_insertion_order() {
        let map = Ordered(vec![("b".to_string(), 1.0), ("a".to_string(), 2.0)]);
        assert_eq!(serde_json::to_string(&map).unwrap(), r#"{"b":1.0,"a":2.0}"#);
    }
}

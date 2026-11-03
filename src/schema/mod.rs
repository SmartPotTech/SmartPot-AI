//! Contrato de la API: validación de peticiones y forma de las respuestas.

pub mod request;
pub mod response;

pub use request::{
    parse_fleet, parse_insight, FieldError, FleetCrop, FleetRequest, InsightRequest, Measures,
    Micros, Outside, Place,
};

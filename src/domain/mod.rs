//! Conocimiento agronómico: perfiles por especie y necesidades de luz.

pub mod placement;
pub mod profiles;

pub use profiles::{profile, CropProfile, Parameter, Range, Values, PROFILES};

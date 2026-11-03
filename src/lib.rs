//! SmartPot-IA: inferencia agronómica de SmartPot en Rust seguro.
//!
//! Sistema experto con encadenamiento hacia adelante, lógica difusa Sugeno, pronóstico Theil-Sen, modelos base de
//! scikit-learn evaluados de forma nativa, agente reactivo con descanso nocturno, consejo de lugar, análisis de flota
//! con K-Means determinista y modelos aprendidos por SmartPot-Trainer con recarga en caliente. Contrato REST y gRPC
//! idéntico al del servicio en Python.
#![deny(unsafe_code)]

pub mod auth;
pub mod config;
pub mod domain;
pub mod engine;
pub mod format;
pub mod grpc;
pub mod http;
pub mod learning;
pub mod ml;
pub mod numeric;
pub mod schema;
pub mod services;
pub mod state;

/// Código generado del contrato `proto/smartpot/v1/ai.proto`.
#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod proto {
    tonic::include_proto!("smartpot.v1");
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

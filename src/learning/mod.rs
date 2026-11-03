//! Aprendizaje continuo del lado de la inferencia: modelos exportados por SmartPot-Trainer, su verificación, la
//! recarga en caliente y el proxy hacia el Trainer.

pub mod bundle;
pub mod features;
pub mod registry;
pub mod trainer;
pub mod verifier;

pub use registry::{Counts, LearnedFacts, Registry, ReloadReport};
pub use trainer::TrainerClient;

//! Motor de inferencia de encadenamiento hacia adelante.
//!
//! Cada ciclo arma la agenda con las reglas cuyas condiciones se cumplen en la memoria de trabajo, dispara la de
//! mayor prioridad (refracción: cada regla se dispara una sola vez) y agrega sus conclusiones como hechos nuevos, que
//! pueden activar otras reglas. Las reglas disparadas se guardan en un conjunto de bits (`fired`), así la refracción
//! y los hechos `rule:<nombre>` cuestan una operación de bits. La traza de disparos es la explicación que ve el
//! usuario.

use crate::domain::placement::{Exposure, Setting};
use crate::domain::Parameter;
use crate::engine::diagnosis::{Severity, Status};
use crate::engine::forecast::Limit;
use crate::schema::Outside;

/// Pronóstico de una variable tal como lo ven las reglas.
#[derive(Debug, Clone, Copy, Default)]
pub struct ForecastFact {
    pub limit: Option<Limit>,
    pub hours: Option<f64>,
}

/// Memoria de trabajo con hechos tipados (el equivalente del diccionario de hechos del servicio en Python).
#[derive(Debug, Clone, Default)]
pub struct Memory {
    pub status: [Option<Status>; 6],
    pub severity: [Option<Severity>; 6],
    pub value: [Option<f64>; 6],
    pub ventilation: Option<f64>,
    pub ph_correction: Option<f64>,
    pub anomaly: Option<f64>,
    pub forecasts: [Option<ForecastFact>; 6],
    pub setting: Option<Setting>,
    pub exposure: Option<Exposure>,
    pub outside: Option<Outside>,
    pub learned_needs_water: Option<f64>,
    pub learned_overheat: Option<f64>,
    pub learned_unusual: Option<f64>,
    // Conclusiones que agregan las reglas.
    pub stress: Option<&'static str>,
    pub risk: Option<&'static str>,
    pub lockout: bool,
    pub sensor_fault: bool,
    pub trend: Option<&'static str>,
    pub raining: bool,
    /// Bit `i` encendido: la regla `i` del catálogo ya se disparó.
    pub fired: u32,
}

impl Memory {
    pub fn status(&self, parameter: Parameter) -> Option<Status> {
        self.status[parameter.index()]
    }

    pub fn critical(&self, parameter: Parameter) -> bool {
        self.severity[parameter.index()] == Some(Severity::Critical)
    }

    pub fn critical_count(&self) -> usize {
        self.severity
            .iter()
            .filter(|s| **s == Some(Severity::Critical))
            .count()
    }

    pub fn has_fired(&self, rule: usize) -> bool {
        self.fired & (1 << rule) != 0
    }
}

/// Una regla del sistema experto.
pub struct Rule {
    pub name: &'static str,
    pub title: &'static str,
    pub certainty: f64,
    pub salience: i32,
    pub condition: fn(&Memory) -> bool,
    pub message: fn(&Memory) -> String,
    pub conclude: fn(&mut Memory),
}

/// Disparo de una regla, con su explicación.
#[derive(Debug, Clone, PartialEq)]
pub struct Firing {
    pub rule: &'static str,
    pub title: &'static str,
    pub message: String,
    pub certainty: f64,
}

pub struct InferenceEngine {
    rules: &'static [Rule],
    /// Índices del catálogo ordenados por prioridad (de mayor a menor, estable como `sorted` de Python).
    order: Vec<usize>,
    max_cycles: usize,
}

impl InferenceEngine {
    pub fn new(rules: &'static [Rule]) -> Self {
        assert!(rules.len() <= 32, "the bit set supports up to 32 rules");
        let mut order: Vec<usize> = (0..rules.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(rules[i].salience));
        Self {
            rules,
            order,
            max_cycles: 50,
        }
    }

    pub fn run(&self, memory: &mut Memory) -> Vec<Firing> {
        let mut trace = Vec::new();
        for _ in 0..self.max_cycles {
            let next = self
                .order
                .iter()
                .copied()
                .find(|&i| !memory.has_fired(i) && (self.rules[i].condition)(memory));
            let Some(index) = next else { break };
            let rule = &self.rules[index];
            (rule.conclude)(memory);
            memory.fired |= 1 << index;
            trace.push(Firing {
                rule: rule.name,
                title: rule.title,
                message: (rule.message)(memory),
                certainty: rule.certainty,
            });
        }
        trace
    }
}

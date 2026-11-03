//! Cliente de SmartPot-Trainer: proxy transparente de las rutas de aprendizaje y conteos para los mensajes.
//!
//! La API sigue hablando solo con la IA. Las lecturas, el estado, el entrenamiento, el olvido de un cultivo y la poda
//! se reenvían tal cual al Trainer (el cuerpo viaja como bytes: las fechas ISO 8601 no se reinterpretan). Si el
//! Trainer no responde, la IA contesta un estado seguro en vez de un error, para no abrir el circuit breaker de la API
//! ni frenar las evaluaciones.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use bytes::Bytes;
use reqwest::{Method, StatusCode};
use serde_json::Value;
use tracing::warn;

use crate::learning::registry::Counts;

/// Vigencia de los conteos por especie, igual que en el servicio en Python.
const COUNTS_TTL_SECONDS: i64 = 30;

#[derive(Debug, Clone, Default)]
struct Snapshot {
    readings: HashMap<String, i64>,
    min_samples: Option<i64>,
}

pub struct TrainerClient {
    base: String,
    token: String,
    http: reqwest::Client,
    snapshot: ArcSwap<Snapshot>,
    refreshed_at: AtomicI64,
    refreshing: AtomicBool,
    loaded: AtomicBool,
}

/// Respuesta del Trainer reenviada a la API.
pub struct Forwarded {
    pub status: StatusCode,
    pub body: Bytes,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl TrainerClient {
    pub fn new(base: &str, token: &str) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(|e| format!("no se pudo preparar el cliente del Trainer: {e}"))?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
            http,
            snapshot: ArcSwap::from_pointee(Snapshot::default()),
            refreshed_at: AtomicI64::new(0),
            refreshing: AtomicBool::new(false),
            loaded: AtomicBool::new(false),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// Reenvía una petición; `None` si el Trainer no respondió.
    pub async fn forward(
        &self,
        method: Method,
        path: &str,
        body: Option<Bytes>,
        timeout: Duration,
    ) -> Option<Forwarded> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .timeout(timeout);
        if let Some(body) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        match request.send().await {
            Ok(response) => {
                let status = response.status();
                match response.bytes().await {
                    Ok(body) => Some(Forwarded { status, body }),
                    Err(error) => {
                        warn!("El Trainer cortó la respuesta de {path}: {error}");
                        None
                    }
                }
            }
            Err(error) => {
                warn!("El Trainer no respondió {path}: {error}");
                None
            }
        }
    }

    /// Actualiza los conteos con el estado del Trainer (lo llama la caché, en segundo plano).
    pub async fn refresh(&self) {
        let response = self
            .forward(
                Method::GET,
                "/v1/learning/status",
                None,
                Duration::from_secs(5),
            )
            .await;
        if let Some(Forwarded { status, body }) = response.filter(|r| r.status.is_success()) {
            if let Ok(status) = serde_json::from_slice::<Value>(&body) {
                self.absorb(&status);
            } else {
                warn!("El Trainer respondió un estado ilegible ({status})");
            }
        }
        self.refreshed_at.store(now(), Ordering::Relaxed);
    }

    /// Toma los conteos de un estado de aprendizaje (`LearningStatus` en camelCase).
    pub fn absorb(&self, status: &Value) {
        let mut snapshot = Snapshot {
            min_samples: status.get("minSamples").and_then(Value::as_i64),
            ..Snapshot::default()
        };
        for item in status
            .get("cropTypes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(kind), Some(readings)) = (
                item.get("cropType").and_then(Value::as_str),
                item.get("readings").and_then(Value::as_i64),
            ) {
                snapshot.readings.insert(kind.to_uppercase(), readings);
            }
        }
        self.snapshot.store(Arc::new(snapshot));
        self.loaded.store(true, Ordering::Release);
        self.refreshed_at.store(now(), Ordering::Relaxed);
    }

    /// Conteos de la especie. Si están vencidos, pide unos nuevos sin esperar la respuesta.
    pub fn counts(self: &Arc<Self>, crop_type: &str) -> Counts {
        if now() - self.refreshed_at.load(Ordering::Relaxed) > COUNTS_TTL_SECONDS
            && !self.refreshing.swap(true, Ordering::AcqRel)
        {
            let this = Arc::clone(self);
            tokio::spawn(async move {
                this.refresh().await;
                this.refreshing.store(false, Ordering::Release);
            });
        }
        if !self.loaded.load(Ordering::Acquire) {
            // Todavía no hay respuesta del Trainer: se usan los datos del propio paquete de modelos.
            return Counts::default();
        }
        let snapshot = self.snapshot.load();
        Counts {
            readings: Some(snapshot.readings.get(crop_type).copied().unwrap_or(0)),
            min_samples: snapshot.min_samples,
        }
    }
}

//! Configuración desde variables de entorno (los mismos nombres del servicio en Python cuando existen).

use std::path::PathBuf;

use ed25519_dalek::VerifyingKey;

use crate::learning::verifier::parse_public_key;

pub const DEFAULT_HTTP_PORT: u16 = 8000;
pub const MIN_TOKEN_LENGTH: usize = 24;

#[derive(Debug, Clone)]
pub struct Settings {
    /// Token que la API envía como `Bearer` (REST) o en el metadato `authorization` (gRPC).
    pub token: String,
    pub http_port: u16,
    /// Socket UNIX de gRPC (misma máquina); `None` lo apaga.
    pub grpc_socket: Option<PathBuf>,
    /// Puerto TCP interno de gRPC; 0 lo apaga.
    pub grpc_tcp_port: u16,
    pub trainer_url: Option<String>,
    pub trainer_token: String,
    /// Carpeta compartida con SmartPot-Trainer donde están los modelos aprendidos.
    pub models_dir: Option<PathBuf>,
    pub trusted_key: Option<VerifyingKey>,
    /// Lecturas con las que el Trainer hace el primer entrenamiento (para el mensaje cuando aún no informa).
    pub min_samples: i64,
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Settings {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Construye la configuración con una función de búsqueda (las pruebas no tocan el entorno real).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let token = clean(get("SMARTPOT_AI_TOKEN")).ok_or("Falta SMARTPOT_AI_TOKEN")?;
        if token.chars().count() < MIN_TOKEN_LENGTH {
            return Err(format!(
                "SMARTPOT_AI_TOKEN debe tener al menos {MIN_TOKEN_LENGTH} caracteres"
            ));
        }
        let port = |name: &str, default: u16| -> Result<u16, String> {
            match clean(get(name)) {
                Some(raw) => raw
                    .parse()
                    .map_err(|_| format!("{name} debe ser un puerto entre 0 y 65535")),
                None => Ok(default),
            }
        };
        let http_port = port("PORT", DEFAULT_HTTP_PORT)?;
        let grpc_tcp_port = match clean(get("GRPC_TCP_PORT")) {
            Some(_) => port("GRPC_TCP_PORT", 0)?,
            None => port("GRPC_PORT", 0)?,
        };
        let grpc_socket = clean(get("AI_GRPC_SOCKET"))
            .or_else(|| clean(get("GRPC_UDS_PATH")))
            .map(PathBuf::from);
        let trainer_url = clean(get("SMARTPOT_TRAINER_URL"));
        if let Some(url) = &trainer_url {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err("SMARTPOT_TRAINER_URL debe empezar por http:// o https://".into());
            }
        }
        let trainer_token = clean(get("SMARTPOT_TRAINER_TOKEN")).unwrap_or_else(|| token.clone());
        let models_dir = match clean(get("MODELS_DIR")) {
            Some(dir) => Some(PathBuf::from(dir)),
            None => match get("DATA_DIR") {
                Some(dir) if dir.trim().is_empty() => None,
                Some(dir) => Some(PathBuf::from(dir.trim()).join("models")),
                None => Some(PathBuf::from("data").join("models")),
            },
        };
        let trusted_key = clean(get("TRUSTED_MODEL_PUBLIC_KEY_HEX"))
            .map(|hex| {
                parse_public_key(&hex).map_err(|e| format!("TRUSTED_MODEL_PUBLIC_KEY_HEX: {e}"))
            })
            .transpose()?;
        let min_samples = match clean(get("LEARNING_MIN_SAMPLES")) {
            Some(raw) => raw
                .parse::<i64>()
                .ok()
                .filter(|v| *v >= 50)
                .ok_or("LEARNING_MIN_SAMPLES debe ser un entero desde 50")?,
            None => 200,
        };
        Ok(Self {
            token,
            http_port,
            grpc_socket,
            grpc_tcp_port,
            trainer_url,
            trainer_token,
            models_dir,
            trusted_key,
            min_samples,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn settings(pairs: &[(&str, &str)]) -> Result<Settings, String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Settings::from_lookup(|name| map.get(name).cloned())
    }

    #[test]
    fn requires_a_long_token() {
        assert!(settings(&[]).is_err());
        assert!(settings(&[("SMARTPOT_AI_TOKEN", "corto")]).is_err());
        let ok = settings(&[("SMARTPOT_AI_TOKEN", "un-token-suficientemente-largo")]).unwrap();
        assert_eq!(ok.http_port, 8000);
        assert_eq!(ok.grpc_tcp_port, 0);
        assert!(ok.grpc_socket.is_none() && ok.trainer_url.is_none());
        assert_eq!(ok.trainer_token, "un-token-suficientemente-largo");
    }

    #[test]
    fn reads_transport_and_trainer_settings() {
        let s = settings(&[
            ("SMARTPOT_AI_TOKEN", "un-token-suficientemente-largo"),
            ("GRPC_UDS_PATH", "/var/run/smartpot/ai.sock"),
            ("GRPC_TCP_PORT", "50051"),
            ("SMARTPOT_TRAINER_URL", "http://trainer-smartpot:8001/"),
            ("DATA_DIR", "/data"),
        ])
        .unwrap();
        assert_eq!(
            s.grpc_socket,
            Some(PathBuf::from("/var/run/smartpot/ai.sock"))
        );
        assert_eq!(s.grpc_tcp_port, 50051);
        assert_eq!(s.models_dir, Some(PathBuf::from("/data").join("models")));
        assert!(settings(&[
            ("SMARTPOT_AI_TOKEN", "un-token-suficientemente-largo"),
            ("SMARTPOT_TRAINER_URL", "ftp://x")
        ])
        .is_err());
        assert!(settings(&[
            ("SMARTPOT_AI_TOKEN", "un-token-suficientemente-largo"),
            ("TRUSTED_MODEL_PUBLIC_KEY_HEX", "zz")
        ])
        .is_err());
        let off = settings(&[
            ("SMARTPOT_AI_TOKEN", "un-token-suficientemente-largo"),
            ("DATA_DIR", ""),
        ])
        .unwrap();
        assert!(off.models_dir.is_none());
    }
}

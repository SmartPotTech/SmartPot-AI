//! Punto de entrada de SmartPot-IA: REST en `PORT` y gRPC por socket UNIX y/o TCP.
#![deny(unsafe_code)]

use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use smartpot_ia::config::{Settings, DEFAULT_HTTP_PORT};
use smartpot_ia::engine::models::BaseModels;
use smartpot_ia::state::AppState;
use smartpot_ia::{grpc, http};

/// `--healthcheck`: sonda de Docker sin curl. Sale con 0 si `/health` responde 200.
async fn healthcheck() -> i32 {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_HTTP_PORT);
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    else {
        return 1;
    };
    match client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => 0,
        _ => 1,
    }
}

async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {},
        () = terminate => {},
    }
}

async fn stopped(mut receiver: watch::Receiver<bool>) {
    let _ = receiver.wait_for(|stop| *stop).await;
}

#[tokio::main]
async fn main() {
    if std::env::args().any(|arg| arg == "--healthcheck") {
        std::process::exit(healthcheck().await);
    }

    let filter = std::env::var("RUST_LOG")
        .ok()
        .or_else(|| {
            std::env::var("LOG_LEVEL")
                .ok()
                .map(|level| level.to_lowercase())
        })
        .unwrap_or_else(|| "info".into());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .init();
    info!("Iniciando SmartPot-IA v{}", smartpot_ia::version());

    let settings = match Settings::from_env() {
        Ok(settings) => settings,
        Err(message) => {
            error!("Configuración inválida: {message}");
            std::process::exit(1);
        }
    };
    let started = std::time::Instant::now();
    let base = match BaseModels::embedded() {
        Ok(base) => base,
        Err(problem) => {
            error!("Modelos base inválidos: {problem}");
            std::process::exit(1);
        }
    };
    info!(
        "Modelos base listos en {:.1} ms: {:?}",
        started.elapsed().as_secs_f64() * 1000.0,
        base.metrics
    );

    let state = match AppState::new(settings, base) {
        Ok(state) => state,
        Err(message) => {
            error!("{message}");
            std::process::exit(1);
        }
    };
    if state.learned.directory().is_some() {
        let report = state.learned.reload(None).await;
        info!(
            "Modelos aprendidos al arrancar: {} cargados, {} rechazados",
            report.reloaded.len(),
            report.rejected.len()
        );
    }
    match &state.trainer {
        Some(trainer) => info!(
            "Aprendizaje delegado en SmartPot-Trainer ({})",
            trainer.base_url()
        ),
        None => warn!(
            "Sin SMARTPOT_TRAINER_URL: las rutas de aprendizaje responden un estado deshabilitado"
        ),
    }

    let (stop, receiver) = watch::channel(false);
    let mut tasks = Vec::new();
    if state.settings.grpc_tcp_port != 0 {
        let address = SocketAddr::from(([0, 0, 0, 0], state.settings.grpc_tcp_port));
        info!("gRPC por TCP en {address}");
        let (state, shutdown) = (state.clone(), stopped(receiver.clone()));
        tasks.push(tokio::spawn(async move {
            if let Err(problem) = grpc::serve_tcp(state, address, shutdown).await {
                error!("gRPC por TCP se detuvo: {problem}");
            }
        }));
    }
    if let Some(path) = state.settings.grpc_socket.clone() {
        #[cfg(unix)]
        {
            info!("gRPC por socket UNIX en {}", path.display());
            let (state, shutdown) = (state.clone(), stopped(receiver.clone()));
            tasks.push(tokio::spawn(async move {
                if let Err(problem) = grpc::serve_unix(state, path, shutdown).await {
                    error!("gRPC por socket UNIX se detuvo: {problem}");
                }
            }));
        }
        #[cfg(not(unix))]
        warn!(
            "Este sistema no tiene sockets UNIX: se ignora {}",
            path.display()
        );
    }

    let address = SocketAddr::from(([0, 0, 0, 0], state.settings.http_port));
    let listener = match TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(problem) => {
            error!("No se pudo abrir {address}: {problem}");
            std::process::exit(1);
        }
    };
    info!("REST escuchando en http://{address}");
    let app = http::router(state);
    let shutdown = async move {
        shutdown_signal().await;
        info!("Deteniendo SmartPot-IA");
        let _ = stop.send(true);
    };
    if let Err(problem) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
    {
        error!("El servidor REST se detuvo: {problem}");
    }
    for task in tasks {
        let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
    }
    info!("SmartPot-IA detenido");
}

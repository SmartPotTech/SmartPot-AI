//! Servidor gRPC (HTTP/2) del contrato `smartpot.v1.SmartPotAIService`, por socket UNIX y/o TCP interno.
//!
//! Lleva el mismo JSON que REST (`JsonBody`): un `0.0` de los sensores llega como `0.0` y un sensor ausente como
//! ausente, y la respuesta es byte a byte la de `POST /v1/insights` o `POST /v1/fleet`. Exige el metadato
//! `authorization: Bearer <SMARTPOT_AI_TOKEN>`. Si gRPC no está disponible, la API usa REST, que pasa por cualquier
//! proxy (incluso HTTP/1.0).

use std::future::Future;
use std::net::SocketAddr;

use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::auth::{bearer_matches, INVALID_TOKEN};
use crate::proto::smart_pot_ai_service_server::{SmartPotAiService, SmartPotAiServiceServer};
use crate::proto::JsonBody;
use crate::services::{self, Invalid};
use crate::state::AppState;

pub struct AiService {
    state: AppState,
}

fn invalid(error: &Invalid) -> Status {
    Status::invalid_argument(error.body().to_string())
}

#[tonic::async_trait]
impl SmartPotAiService for AiService {
    async fn evaluate_insight(
        &self,
        request: Request<JsonBody>,
    ) -> Result<Response<JsonBody>, Status> {
        let json =
            services::insight(&self.state, &request.into_inner().json).map_err(|e| invalid(&e))?;
        Ok(Response::new(JsonBody { json }))
    }

    async fn analyze_fleet(
        &self,
        request: Request<JsonBody>,
    ) -> Result<Response<JsonBody>, Status> {
        let json = services::fleet(&request.into_inner().json).map_err(|e| invalid(&e))?;
        Ok(Response::new(JsonBody { json }))
    }
}

/// Interceptor que valida el token en tiempo constante.
#[derive(Clone)]
pub struct TokenCheck {
    token: std::sync::Arc<str>,
}

impl tonic::service::Interceptor for TokenCheck {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        let header = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        if bearer_matches(header, &self.token) {
            Ok(request)
        } else {
            Err(Status::unauthenticated(INVALID_TOKEN))
        }
    }
}

pub fn service(
    state: AppState,
) -> InterceptedService<SmartPotAiServiceServer<AiService>, TokenCheck> {
    let check = TokenCheck {
        token: state.settings.token.clone().into(),
    };
    SmartPotAiServiceServer::with_interceptor(AiService { state }, check)
}

/// gRPC por TCP interno.
pub async fn serve_tcp(
    state: AppState,
    address: SocketAddr,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), tonic::transport::Error> {
    Server::builder()
        .add_service(service(state))
        .serve_with_shutdown(address, shutdown)
        .await
}

/// gRPC por socket UNIX: reemplaza un socket viejo y lo deja accesible a los contenedores que comparten el volumen.
#[cfg(unix)]
pub async fn serve_unix(
    state: AppState,
    path: std::path::PathBuf,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::symlink_metadata(&path).is_ok() {
        std::fs::remove_file(&path)?;
    }
    let listener = tokio::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666))?;
    let incoming = tokio_stream::wrappers::UnixListenerStream::new(listener);
    let result = Server::builder()
        .add_service(service(state))
        .serve_with_incoming_shutdown(incoming, shutdown)
        .await;
    let _ = std::fs::remove_file(&path);
    result.map_err(Into::into)
}

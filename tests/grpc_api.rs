//! gRPC: el mismo JSON que REST, con el token en los metadatos, por TCP y por socket UNIX.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use serde_json::{json, Value};
use smartpot_ia::grpc;
use smartpot_ia::http::router;
use smartpot_ia::proto::smart_pot_ai_service_client::SmartPotAiServiceClient;
use smartpot_ia::proto::JsonBody;
use smartpot_ia::state::AppState;
use tonic::transport::Channel;
use tonic::Code;

use common::{call, fixture, state, state_with, TOKEN};

fn request(body: &Value, token: &str) -> tonic::Request<JsonBody> {
    let mut request = tonic::Request::new(JsonBody {
        json: serde_json::to_vec(body).unwrap(),
    });
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
}

async fn tcp_address(state: AppState) -> SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    tokio::spawn(grpc::serve_tcp(state, address, std::future::pending()));
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(address).await.is_ok() {
            return address;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the gRPC server did not start");
}

async fn tcp_client_with(state: AppState) -> SmartPotAiServiceClient<Channel> {
    let address = tcp_address(state).await;
    for _ in 0..50 {
        if let Ok(client) = SmartPotAiServiceClient::connect(format!("http://{address}")).await {
            return client;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the gRPC client did not connect");
}

async fn tcp_client() -> SmartPotAiServiceClient<Channel> {
    tcp_client_with(state()).await
}

fn request_bytes(body: &[u8], token: &str) -> tonic::Request<JsonBody> {
    let mut request = tonic::Request::new(JsonBody {
        json: body.to_vec(),
    });
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
}

#[tokio::test]
async fn grpc_answers_exactly_what_rest_answers() {
    let mut client = tcp_client().await;
    let app = router(state());
    let cases = fixture("parity_insights.json");
    for case in cases
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["status"] == 200)
        .take(60)
    {
        let body = &case["request"];
        let (_, rest) = call(&app, "POST", "/v1/insights", Some(body), Some(TOKEN)).await;
        let reply = client
            .evaluate_insight(request(body, TOKEN))
            .await
            .unwrap()
            .into_inner();
        let over_grpc: Value = serde_json::from_slice(&reply.json).unwrap();
        assert_eq!(over_grpc, rest);
        assert_eq!(
            over_grpc, case["response"],
            "also matches the parity fixture"
        );
    }
    let fleets = fixture("parity_fleet.json");
    for case in fleets
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["status"] == 200)
        .take(30)
    {
        let reply = client
            .analyze_fleet(request(&case["request"], TOKEN))
            .await
            .unwrap()
            .into_inner();
        let over_grpc: Value = serde_json::from_slice(&reply.json).unwrap();
        assert_eq!(over_grpc, case["response"]);
    }
}

#[tokio::test]
async fn grpc_keeps_zero_readings_and_requires_the_token() {
    let mut client = tcp_client().await;
    let night = json!({"cropType": "LETTUCE", "measures": {"temperature": 0.0, "brightness": 0.0}, "localHour": 2});
    let reply = client
        .evaluate_insight(request(&night, TOKEN))
        .await
        .unwrap()
        .into_inner();
    let body: Value = serde_json::from_slice(&reply.json).unwrap();
    assert_eq!(body["diagnosis"][0]["value"], 0.0);
    assert_eq!(body["diagnosis"][1]["status"], "REST");

    let denied = client
        .evaluate_insight(request(&night, "a-wrong-test-token-value"))
        .await
        .unwrap_err();
    assert_eq!(denied.code(), Code::Unauthenticated);
    assert_eq!(denied.message(), "Invalid service token");
    let missing = client
        .analyze_fleet(tonic::Request::new(JsonBody {
            json: b"{}".to_vec(),
        }))
        .await
        .unwrap_err();
    assert_eq!(missing.code(), Code::Unauthenticated);

    let invalid = client
        .evaluate_insight(request(
            &json!({"cropType": "CACTUS", "measures": {}}),
            TOKEN,
        ))
        .await
        .unwrap_err();
    assert_eq!(invalid.code(), Code::InvalidArgument);
    let detail: Value = serde_json::from_str(invalid.message()).unwrap();
    assert_eq!(detail["detail"], "Invalid data");
}

#[tokio::test]
async fn grpc_derives_the_local_hour_with_the_configured_timezone() {
    let state = state_with(&[("AI_TIMEZONE", "Asia/Tokyo")]);
    let app = router(state.clone());
    let mut client = tcp_client_with(state).await;

    for (measured_at, expected_status, expects_light) in [
        // 04:00 UTC = 13:00 en Tokio: hay luz insuficiente y el agente puede encenderla.
        ("2026-11-02T04:00:00Z", "LOW", true),
        // 15:30 UTC = 00:30 del día siguiente: descanso nocturno.
        ("2026-11-02T15:30:00Z", "REST", false),
    ] {
        let body = json!({
            "cropType": "LETTUCE",
            "measures": {"brightness": 0.0, "measuredAt": measured_at},
            "actuators": ["UV_LIGHT"],
        });
        let (status, rest) = call(&app, "POST", "/v1/insights", Some(&body), Some(TOKEN)).await;
        assert_eq!(status, axum::http::StatusCode::OK, "unexpected REST status");
        let reply = client
            .evaluate_insight(request(&body, TOKEN))
            .await
            .unwrap()
            .into_inner();
        let over_grpc: Value = serde_json::from_slice(&reply.json).unwrap();
        assert_eq!(
            over_grpc, rest,
            "REST and gRPC must agree for {measured_at}"
        );
        assert_eq!(over_grpc["diagnosis"][0]["status"], expected_status);
        assert_eq!(
            over_grpc["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|action| action["actuator"] == "UV_LIGHT" && action["action"] == "ACTIVATE"),
            expects_light,
            "unexpected light decision for {measured_at}"
        );
    }
}

#[tokio::test]
async fn grpc_rejects_unknown_methods_and_malformed_payloads_like_rest() {
    let app = router(state());

    // 422 de REST y InvalidArgument de gRPC llevan exactamente el mismo cuerpo.
    let invalid = json!({"cropType": "LETTUCE", "measures": {"temperature": 61}});
    let (status, rest) = call(&app, "POST", "/v1/insights", Some(&invalid), Some(TOKEN)).await;
    assert_eq!(
        status,
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "unexpected REST status"
    );
    assert_eq!(rest["detail"], "Invalid data");
    let mut client = tcp_client().await;
    let denied = client
        .evaluate_insight(request(&invalid, TOKEN))
        .await
        .unwrap_err();
    assert_eq!(denied.code(), Code::InvalidArgument);
    let grpc_body: Value = serde_json::from_str(denied.message()).unwrap();
    assert_eq!(grpc_body, rest);

    // JSON ilegible: REST 422 y gRPC InvalidArgument, no un error interno.
    let malformed = b"{not json";
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/insights")
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(axum::body::Body::from(malformed.as_slice()))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    let unreadable = client
        .evaluate_insight(request_bytes(malformed, TOKEN))
        .await
        .unwrap_err();
    assert_eq!(unreadable.code(), Code::InvalidArgument);
    let body: Value = serde_json::from_str(unreadable.message()).unwrap();
    assert_eq!(
        body["errors"][0]["message"],
        "Request body is not valid JSON"
    );

    // Una ruta o método que el contrato no expone es Unimplemented, el equivalente gRPC de 404/405.
    let address = tcp_address(state()).await;
    let channel = Channel::from_shared(format!("http://{address}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut grpc_client = tonic::client::Grpc::new(channel);
    grpc_client.ready().await.unwrap();
    let codec = tonic_prost::ProstCodec::<JsonBody, JsonBody>::default();
    let path: tonic::codegen::http::uri::PathAndQuery =
        "/smartpot.v1.SmartPotAIService/DeleteCrop".parse().unwrap();
    // Igual que REST, el token se exige antes de decidir que la ruta no existe.
    let denied = grpc_client
        .unary(
            tonic::Request::new(JsonBody {
                json: b"{}".to_vec(),
            }),
            path.clone(),
            codec.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(denied.code(), Code::Unauthenticated);
    assert_eq!(denied.message(), "Invalid service token");
    grpc_client.ready().await.unwrap();
    let missing = grpc_client
        .unary(request_bytes(b"{}", TOKEN), path, codec)
        .await
        .unwrap_err();
    assert_eq!(missing.code(), Code::Unimplemented);
}

#[cfg(unix)]
#[tokio::test]
async fn grpc_over_a_unix_socket() {
    use hyper_util::rt::TokioIo;
    use tonic::transport::{Endpoint, Uri};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ai.sock");
    tokio::spawn(grpc::serve_unix(
        state(),
        path.clone(),
        std::future::pending(),
    ));
    for _ in 0..50 {
        if path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let socket = path.clone();
    let channel = Endpoint::try_from("http://localhost")
        .unwrap()
        .connect_with_connector(tower::service_fn(move |_: Uri| {
            let socket = socket.clone();
            async move {
                Ok::<_, std::io::Error>(TokioIo::new(
                    tokio::net::UnixStream::connect(socket).await?,
                ))
            }
        }))
        .await
        .unwrap();
    let mut client = SmartPotAiServiceClient::new(channel);
    let body = json!({"cropType": "BASIL", "measures": {"soilMoisture": 30.0}, "actuators": ["WATER_PUMP"]});
    let reply = client
        .evaluate_insight(request(&body, TOKEN))
        .await
        .unwrap()
        .into_inner();
    let answer: Value = serde_json::from_slice(&reply.json).unwrap();
    assert_eq!(answer["actions"][0]["actuator"], "WATER_PUMP");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o666
    );
}

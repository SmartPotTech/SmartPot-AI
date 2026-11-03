//! gRPC: el mismo JSON que REST, con el token en los metadatos, por TCP y por socket UNIX.

mod common;

use std::time::Duration;

use serde_json::{json, Value};
use smartpot_ia::grpc;
use smartpot_ia::http::router;
use smartpot_ia::proto::smart_pot_ai_service_client::SmartPotAiServiceClient;
use smartpot_ia::proto::JsonBody;
use tonic::transport::Channel;
use tonic::Code;

use common::{call, fixture, state, TOKEN};

fn request(body: &Value, token: &str) -> tonic::Request<JsonBody> {
    let mut request = tonic::Request::new(JsonBody {
        json: serde_json::to_vec(body).unwrap(),
    });
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
}

async fn tcp_client() -> SmartPotAiServiceClient<Channel> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    tokio::spawn(grpc::serve_tcp(state(), address, std::future::pending()));
    for _ in 0..50 {
        if let Ok(client) = SmartPotAiServiceClient::connect(format!("http://{address}")).await {
            return client;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("el servidor gRPC no arrancó");
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
            "también coincide con el servicio en Python"
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
        .evaluate_insight(request(&night, "un-token-equivocado-de-prueba"))
        .await
        .unwrap_err();
    assert_eq!(denied.code(), Code::Unauthenticated);
    assert_eq!(denied.message(), "Token de servicio inválido");
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
    assert_eq!(detail["detail"], "Datos inválidos");
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

import json
from concurrent import futures

import grpc
import pytest

from app.grpc_server.server import SmartPotAIServicer, TokenInterceptor
from app.proto.smartpot.v1 import ai_pb2, ai_pb2_grpc
from tests.conftest import TOKEN
from tests.test_api import PAYLOAD


@pytest.fixture
def stub(client):
    # Mismos modelos que la app REST, para comparar las dos vías.
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=2), interceptors=[TokenInterceptor(TOKEN)])
    servicer = SmartPotAIServicer(client.app.state.models, getattr(client.app.state, "learning", None))
    ai_pb2_grpc.add_SmartPotAIServiceServicer_to_server(servicer, server)
    port = server.add_insecure_port("127.0.0.1:0")
    server.start()
    channel = grpc.insecure_channel(f"127.0.0.1:{port}")
    yield ai_pb2_grpc.SmartPotAIServiceStub(channel)
    channel.close()
    server.stop(grace=None)


def _call(rpc, body: dict, token: str | None = TOKEN) -> dict:
    metadata = [("authorization", f"Bearer {token}")] if token else []
    response = rpc(ai_pb2.JsonBody(json=json.dumps(body).encode()), metadata=metadata, timeout=30)
    return json.loads(response.json)


def test_grpc_answers_exactly_like_rest(client, auth, stub):
    rest = client.post("/v1/insights", json=PAYLOAD, headers=auth).json()
    via_grpc = _call(stub.EvaluateInsight, PAYLOAD)
    for field in ("health", "diagnosis", "conclusions", "predictions", "actions", "forecasts", "summary"):
        assert via_grpc.get(field) == rest.get(field), field


def test_grpc_keeps_zero_readings_and_missing_sensors(client, auth, stub):
    # 0 °C y 0 lux son lecturas reales (helada, noche); un sensor ausente no se inventa.
    payload = {**PAYLOAD, "measures": {"temperature": 0, "brightness": 0, "humidity": 70}}
    rest = client.post("/v1/insights", json=payload, headers=auth).json()
    via_grpc = _call(stub.EvaluateInsight, payload)
    assert via_grpc["diagnosis"] == rest["diagnosis"]
    assert any(d["parameter"] == "temperature" for d in via_grpc["diagnosis"])
    assert not any(d["parameter"] == "ph" for d in via_grpc["diagnosis"])


def test_grpc_fleet_matches_rest(client, auth, stub):
    crops = [{"id": f"c{i}", "name": f"Lechuga {i}", "cropType": "LETTUCE", "measures": PAYLOAD["measures"]}
             for i in range(3)]
    fleet = {"crops": crops, "localHour": 14}
    rest = client.post("/v1/fleet", json=fleet, headers=auth)
    assert rest.status_code == 200
    assert _call(stub.AnalyzeFleet, fleet) == rest.json()


def test_grpc_requires_the_service_token(stub):
    with pytest.raises(grpc.RpcError) as error:
        _call(stub.EvaluateInsight, PAYLOAD, token="otro")
    assert error.value.code() == grpc.StatusCode.UNAUTHENTICATED
    with pytest.raises(grpc.RpcError) as error:
        _call(stub.EvaluateInsight, PAYLOAD, token=None)
    assert error.value.code() == grpc.StatusCode.UNAUTHENTICATED


def test_grpc_rejects_an_invalid_body(stub):
    with pytest.raises(grpc.RpcError) as error:
        _call(stub.EvaluateInsight, {"measures": "no es un objeto"})
    assert error.value.code() == grpc.StatusCode.INVALID_ARGUMENT

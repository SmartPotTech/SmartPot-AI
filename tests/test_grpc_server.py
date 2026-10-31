from app.engine.models import ModelRegistry
from app.grpc_server.server import SmartPotAIServicer
from app.proto.smartpot.v1 import ai_pb2


def test_grpc_evaluate_insight_direct():
    models = ModelRegistry(seed=42)
    servicer = SmartPotAIServicer(models=models, learning=None)

    measures_proto = ai_pb2.Measures(
        temperature=23.5,
        humidity=65.0,
        ph=6.2,
        tds=850.0,
        soil_moisture=70.0,
        brightness=15000.0,
        atmosphere=1013.0,
    )
    req = ai_pb2.InsightRequest(
        crop_id="test-crop-001",
        crop_type="LETTUCE",
        measures=measures_proto,
        local_hour=14,
    )

    res = servicer.EvaluateInsight(req, None)
    assert res.health.index > 0
    assert len(res.health.label) > 0
    assert res.health.level in ("EXCELLENT", "GOOD", "FAIR", "POOR", "CRITICAL")
    assert res.evaluation_latency_nanos > 0
    assert len(res.diagnosis) > 0


def test_grpc_fleet_analysis_direct():
    models = ModelRegistry(seed=42)
    servicer = SmartPotAIServicer(models=models, learning=None)

    crops = [
        ai_pb2.FleetCropItem(
            crop_id="c1",
            crop_type="LETTUCE",
            measures=ai_pb2.Measures(temperature=24.0, humidity=60.0, ph=6.0, tds=800.0, soil_moisture=70.0),
            health_index=85,
        ),
        ai_pb2.FleetCropItem(
            crop_id="c2",
            crop_type="LETTUCE",
            measures=ai_pb2.Measures(temperature=35.0, humidity=20.0, ph=5.0, tds=1200.0, soil_moisture=25.0),
            health_index=35,
        ),
    ]
    req = ai_pb2.FleetRequest(crops=crops)
    res = servicer.AnalyzeFleet(req, None)
    assert res.cluster_count >= 0
    assert "Analizados 2 cultivos" in res.fleet_summary

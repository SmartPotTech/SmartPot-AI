import os
import tempfile

import pytest

os.environ.setdefault("SMARTPOT_AI_TOKEN", "test-token-for-smartpot-ai-service")
os.environ.setdefault("DATA_DIR", tempfile.mkdtemp(prefix="smartpot-ai-"))
os.environ.setdefault("LEARNING_CHECK_SECONDS", "0")
os.environ.setdefault("LEARNING_SEPARATE_PROCESS", "false")

from fastapi.testclient import TestClient  # noqa: E402

from app.engine.models import ModelRegistry  # noqa: E402
from app.main import create_app  # noqa: E402

TOKEN = os.environ["SMARTPOT_AI_TOKEN"]


@pytest.fixture(scope="session")
def models() -> ModelRegistry:
    return ModelRegistry(seed=42)


@pytest.fixture(scope="session")
def client():
    with TestClient(create_app()) as test_client:
        yield test_client


@pytest.fixture
def auth() -> dict[str, str]:
    return {"Authorization": f"Bearer {TOKEN}"}

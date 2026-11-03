# SmartPot-IA-Rust

## Estado del Proyecto

[![Rust CI](https://github.com/SmartPotTech/SmartPot-IA-Rust/actions/workflows/ci.yml/badge.svg)](https://github.com/SmartPotTech/SmartPot-IA-Rust/actions/workflows/ci.yml)
[![CodeQL Advanced](https://github.com/SmartPotTech/SmartPot-IA-Rust/actions/workflows/codeql.yml/badge.svg)](https://github.com/SmartPotTech/SmartPot-IA-Rust/actions/workflows/codeql.yml)
[![Publish Docker Images](https://github.com/SmartPotTech/SmartPot-IA-Rust/actions/workflows/packaging.yml/badge.svg)](https://github.com/SmartPotTech/SmartPot-IA-Rust/actions/workflows/packaging.yml)
[![Snyk Security Scan](https://github.com/SmartPotTech/SmartPot-IA-Rust/actions/workflows/snyk.yml/badge.svg)](https://github.com/SmartPotTech/SmartPot-IA-Rust/actions/workflows/snyk.yml)

## Descripción

SmartPot-IA-Rust es el **asistente inteligente del stack nuevo** de SmartPot (`demo-beta`): recibe la última lectura
de un cultivo, su historial, los actuadores, el lugar y el clima de afuera, y devuelve el diagnóstico completo. Es la
inferencia; el entrenamiento lo hace [SmartPot-Trainer](https://github.com/SmartPotTech/SmartPot-Trainer). El stack
actual (producción, `demo` y `dev`) sigue con [SmartPot-AI](https://github.com/SmartPotTech/SmartPot-AI) en Python.

Habla **el mismo contrato** que SmartPot-AI, campo por campo y texto por texto: la API no nota el cambio.

| Técnica | Qué resuelve | Dónde vive |
|---|---|---|
| **Sistema experto** | 21 reglas con encadenamiento hacia adelante (estrés térmico, hongos, bloqueo de nutrientes, falla de sensor, lluvia, tendencia…) | `src/engine/rules.rs`, `expert.rs` |
| **Lógica difusa** | Índice de salud de 0 a 100 (Sugeno de orden cero, trapecios sin ramas, peor caso crítico) | `src/engine/fuzzy.rs` |
| **Pronóstico** | Tendencia por variable con Theil-Sen (mediana por selección parcial en O(M)) y horas hasta el límite | `src/engine/forecast.rs` |
| **Modelos base** | Regresión logística, red MLP e Isolation Forest de SmartPot-AI, evaluados de forma nativa | `src/engine/models.rs`, `src/ml/` |
| **Agente reactivo** | Acciones sobre los actuadores del cultivo; respeta el descanso nocturno de 22:00 a 6:00 | `src/engine/agent.rs` |
| **Consejo de lugar** | Si el lugar le sirve a la especie y cuándo conviene moverla | `src/engine/placement.rs` |
| **Análisis de flota** | Ranking, problemas compartidos, grupos K-Means con k por silueta y acciones en bloque | `src/engine/fleet.rs`, `src/ml/kmeans.rs` |
| **Lo aprendido** | Modelos del Trainer (campeones, K-Means, Isolation Forest, LoRA) con firma Ed25519 y recarga en caliente | `src/learning/` |

También trae el VPD foliar (Buck 1981) y un filtro de Kalman 1D con compuerta de 3σ (`src/engine/vpd.rs`,
`ekf.rs`) como utilidades probadas; no cambian la respuesta, para no romper la paridad.

## Paridad con SmartPot-AI

La paridad se comprueba con el propio servicio en Python como oráculo (`tests/fixtures`, regenerables con `tools/`):

| Prueba | Casos | Resultado |
|---|---|---|
| `POST /v1/insights` | 418 peticiones (405 válidas y 13 inválidas), con ceros nocturnos, sensores ausentes, historial, lugar, clima y reglas poco frecuentes | Mismas respuestas |
| `POST /v1/fleet` | 156 flotas de 1 a 12 cultivos (150 válidas) | Mismas respuestas, grupos incluidos |
| Lo aprendido | 40 evaluaciones con modelos que entrenó el Trainer | Mismas respuestas |
| Modelos base | 400 + 400 entradas contra scikit-learn | Diferencia ≤ 1e-12 |

Para lograrlo, Rust repite las operaciones de Python en el mismo orden: `round` con mitad al par, la suma compensada
de `sum()` (Python 3.12+), la suma por pares de NumPy, la conversión a `f32` de los árboles de scikit-learn y, en la
flota, el K-Means de scikit-learn con su generador MT19937 y la misma semilla.

## Contrato

REST en el puerto `8000` (HTTP/1.0, HTTP/1.1 y HTTP/2 sin TLS). `/health` es público; todo `/v1/*` exige
`Authorization: Bearer <SMARTPOT_AI_TOKEN>`, comparado en tiempo constante.

| Método | Ruta | Qué hace |
|---|---|---|
| GET | `/health` | Estado (`UP`), versión, aprendizaje y especies con modelos aprendidos |
| POST | `/v1/insights` | Evalúa una lectura (`InsightRequest` → `InsightResponse`) |
| POST | `/v1/fleet` | Analiza los cultivos de una cuenta (`FleetRequest` → `FleetResponse`) |
| GET | `/v1/crop-profiles`, `/v1/profiles`, `/v1/profiles/{cropType}` | Base de conocimiento por especie |
| GET | `/v1/models` | Exactitud de los modelos base |
| POST | `/v1/learning/reload` | Recarga en caliente los modelos del Trainer (`{"cropType": "LETTUCE"}` o todos) |
| POST, GET, POST, DELETE, POST | `/v1/learning/readings`, `/status`, `/train`, `/crops/{cropId}`, `/prune` | Se reenvían tal cual a SmartPot-Trainer |

Especies: `LETTUCE`, `TOMATO`, `STRAWBERRY`, `BASIL`, `SPINACH` y `PEPPER` (otra especie responde 422, como en
Python). Actuadores: `WATER_PUMP`, `UV_LIGHT`, `FAN`, `HUMIDIFIER`, `PH_DOSER` y `NUTRIENT_DOSER`.

**gRPC** (`proto/smartpot/v1/ai.proto`, el mismo de `.github/proto`): `EvaluateInsight` y `AnalyzeFleet` llevan el
JSON de REST en `JsonBody`, así un `0.0` (helada, noche) llega como `0.0` y un sensor ausente como ausente, y la
respuesta es la de REST byte a byte. Escucha en el socket UNIX `AI_GRPC_SOCKET` (permisos 0666 para los contenedores
que comparten el volumen) y/o en TCP `GRPC_TCP_PORT`, con el metadato `authorization: Bearer <token>`.

Errores con la forma de FastAPI: 401 `{"detail": "Token de servicio inválido"}`, 422
`{"detail": "Datos inválidos", "errors": [{"field", "message"}]}`, 404 y 405.

## Aprendizaje continuo

SmartPot-Trainer entrena por especie y escribe en la carpeta compartida (`DATA_DIR/models`):

- `{ESPECIE}_inference.json`: los campeones (escalado y parámetros), K-Means, Isolation Forest, LoRA
  (`W = W0 + (α/r)·B·A`) y 16 casos de control evaluados con scikit-learn.
- `{ESPECIE}_manifest.json`: hash BLAKE2b de cada archivo y firma Ed25519 del JSON canónico.

Al recibir `POST /v1/learning/reload` (lo llama el Trainer al terminar), la IA lee el archivo, verifica el manifiesto y
cambia la especie de una vez, sin pausar evaluaciones (`ArcSwap`). Con `TRUSTED_MODEL_PUBLIC_KEY_HEX` solo acepta lo
firmado con esa clave (falla cerrada: sin manifiesto, con otra firma o con otro hash, rechaza). Al cargar evalúa los
casos de control: un modelo que no da lo mismo que en Python se deshabilita.

Sin `SMARTPOT_TRAINER_URL`, las rutas de aprendizaje responden un estado deshabilitado seguro. Si el Trainer no
responde, lecturas, olvido y poda devuelven un resultado vacío en vez de un error, para no abrir el circuit breaker de
la API; la evaluación nunca depende del Trainer.

## Variables de entorno

| Variable | Por defecto | Uso |
|---|---|---|
| `SMARTPOT_AI_TOKEN` | — | Token de servicio (mínimo 24 caracteres). Obligatorio |
| `PORT` | `8000` | Puerto REST |
| `AI_GRPC_SOCKET` (o `GRPC_UDS_PATH`) | vacío | Socket UNIX de gRPC |
| `GRPC_TCP_PORT` | `0` | Puerto TCP de gRPC (0 lo apaga) |
| `SMARTPOT_TRAINER_URL` | vacío | URL de SmartPot-Trainer |
| `SMARTPOT_TRAINER_TOKEN` | `SMARTPOT_AI_TOKEN` | Token para llamar al Trainer |
| `DATA_DIR` | `data` | Carpeta compartida; los modelos se leen de `DATA_DIR/models` (vacío = sin modelos) |
| `TRUSTED_MODEL_PUBLIC_KEY_HEX` | vacío | Clave pública Ed25519 del Trainer |
| `LEARNING_MIN_SAMPLES` | `200` | Lecturas del primer entrenamiento, para el mensaje mientras el Trainer no informa |
| `RUST_LOG` | `info` | Nivel de registro |

## Despliegue en un comando

La imagen es pública: `ghcr.io/smartpottech/smartpot-ia-rust`. El stack nuevo completo, con este servicio, se levanta
sin descargar el código:

```bash
curl -fsSL https://raw.githubusercontent.com/SmartPotTech/.github/main/docker/demo-beta/compose.yaml | docker compose -f - up -d --wait
```

Abre http://localhost:5173 e ingresa con `demo@smartpot.app` / `SmartPot2026`. Se detiene con
`docker compose -p smartpot-demo-beta down`. Más detalle en la
[demo beta](https://github.com/SmartPotTech/.github/tree/main/docker/demo-beta).

## Desarrollo y pruebas

Requisitos: Rust 1.88 o superior (no hace falta `protoc`: el contrato se compila con protox).

```bash
cp .env.example .env    # define SMARTPOT_AI_TOKEN
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --release
```

Las pruebas cubren perfiles, lógica difusa, Theil-Sen, reglas, descanso nocturno del agente, K-Means, LoRA, VPD, el
filtro de Kalman, la firma Ed25519 y el hash BLAKE2b, REST, gRPC por TCP y por socket UNIX, el proxy al Trainer y la
paridad con Python.

### Regenerar los casos de paridad y los modelos base

Con los repositorios clonados uno junto al otro:

```bash
uv run --project ../SmartPot-AI python tools/export_base_models.py ../SmartPot-AI
uv run --project ../SmartPot-AI python tools/generate_parity_fixtures.py ../SmartPot-AI
uv run --project ../SmartPot-Trainer python tools/generate_learned_fixtures.py train ../SmartPot-Trainer
uv run --project ../SmartPot-AI python tools/generate_learned_fixtures.py expect ../SmartPot-AI
```

`models/base_models.json` queda dentro del binario: los modelos base cargan al arrancar en unos milisegundos, sin
entrenar.

### Imagen Docker

```bash
docker build -t smartpot-ia-rust .
docker pull ghcr.io/smartpottech/smartpot-ia-rust:latest
```

Compilación en `rust:1.98-bookworm` y ejecución en `debian:bookworm-slim` actualizado, con el usuario sin privilegios
`10001`. Admite `read_only: true`, `cap_drop: ALL` y `no-new-privileges:true`; la sonda de salud es el propio binario
(`smartpot-ia --healthcheck`), sin `curl`. Cada cambio en `main` pasa por el CI y publica la imagen en GHCR (y en
Docker Hub si el repositorio tiene credenciales).

## Documentación

[`docs/SmartPot_IA_Rust_Documentation.md`](docs/SmartPot_IA_Rust_Documentation.md) explica la arquitectura, cómo razona
una evaluación, la paridad numérica y el ciclo de los modelos aprendidos, con sus diagramas en
[`docs/diagrams`](docs/diagrams).

## Licencia

Este proyecto está bajo la licencia MIT. Consulta el archivo [LICENSE](LICENSE) para más detalles.

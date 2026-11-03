FROM rust:1.98-bookworm AS builder

WORKDIR /app

# Dependencias primero: se reconstruyen solo si cambia Cargo.lock.
COPY Cargo.toml Cargo.lock build.rs ./
COPY proto ./proto
RUN mkdir src \
    && echo "pub fn version() -> &'static str { env!(\"CARGO_PKG_VERSION\") }" > src/lib.rs \
    && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY models ./models
COPY src ./src
RUN touch src/lib.rs src/main.rs && cargo build --release --locked --bin smartpot-ia

FROM debian:bookworm-slim

LABEL org.opencontainers.image.title="SmartPot IA (Rust)" \
      org.opencontainers.image.description="Inferencia agronómica de SmartPot en Rust: sistema experto, lógica difusa, modelos, agente, pronóstico y flota con REST y gRPC" \
      org.opencontainers.image.source="https://github.com/SmartPotTech/SmartPot-IA-Rust" \
      org.opencontainers.image.licenses="MIT"

RUN apt-get update && apt-get upgrade -y --no-install-recommends && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 appuser \
    && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin appuser

COPY --from=builder /app/target/release/smartpot-ia /usr/local/bin/smartpot-ia

USER 10001:10001

# Los modelos aprendidos se leen de /data/models (volumen compartido con SmartPot-Trainer, montado en solo lectura).
ENV PORT=8000 \
    DATA_DIR=/data \
    RUST_LOG=info

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/smartpot-ia", "--healthcheck"]

ENTRYPOINT ["/usr/local/bin/smartpot-ia"]

# syntax=docker/dockerfile:1

FROM rust:1.96-bookworm AS builder
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rustup target add x86_64-unknown-linux-musl \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock askama.toml ./
COPY src ./src
COPY templates ./templates
COPY assets ./assets
COPY tests ./tests

RUN cargo build --release --target x86_64-unknown-linux-musl \
    && cp target/x86_64-unknown-linux-musl/release/webhook-mocker /webhook-mocker

FROM gcr.io/distroless/static-debian13:nonroot
COPY --from=builder /webhook-mocker /webhook-mocker
ENV WM_LISTEN=0.0.0.0:5080
EXPOSE 5080
USER nonroot
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/webhook-mocker", "healthcheck"]
ENTRYPOINT ["/webhook-mocker"]

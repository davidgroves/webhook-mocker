# syntax=docker/dockerfile:1

FROM rust:1.96-bookworm AS builder
ARG TARGETARCH
WORKDIR /app

# TARGETARCH is set by BuildKit (docker buildx). Fall back to the builder host for plain builds.
RUN arch="${TARGETARCH}" \
    && if [ -z "$arch" ]; then \
         case "$(uname -m)" in \
           x86_64) arch=amd64 ;; \
           aarch64|arm64) arch=arm64 ;; \
           *) echo "unsupported arch: $(uname -m)" >&2; exit 1 ;; \
         esac; \
       fi \
    && case "$arch" in \
         amd64) echo "x86_64-unknown-linux-musl" > /rust_target ;; \
         arm64) echo "aarch64-unknown-linux-musl" > /rust_target ;; \
         *) echo "unsupported TARGETARCH: $arch" >&2; exit 1 ;; \
       esac \
    && apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rustup target add "$(cat /rust_target)" \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock askama.toml ./
COPY src ./src
COPY templates ./templates
COPY assets ./assets
COPY tests ./tests

RUN TARGET="$(cat /rust_target)" \
    && cargo build --release --locked --target "$TARGET" \
    && cp "target/${TARGET}/release/webhook-mocker" /webhook-mocker

FROM gcr.io/distroless/static-debian13:nonroot
COPY --from=builder /webhook-mocker /webhook-mocker
ENV WM_LISTEN=0.0.0.0:5080
EXPOSE 5080
USER nonroot
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/webhook-mocker", "healthcheck"]
ENTRYPOINT ["/webhook-mocker"]

FROM mcr.microsoft.com/devcontainers/rust:1-1-bookworm

# Match the release Dockerfile target and keep common Playwright OS libs available.
# The microsoft image pins an older rustc; bump to current stable so Cargo.lock resolves.
RUN apt-get update \
    && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y --no-install-recommends \
        musl-tools \
        # Chromium/Playwright runtime libraries
        libasound2 \
        libatk-bridge2.0-0 \
        libatk1.0-0 \
        libcups2 \
        libdbus-1-3 \
        libdrm2 \
        libgbm1 \
        libgtk-3-0 \
        libnspr4 \
        libnss3 \
        libx11-xcb1 \
        libxcomposite1 \
        libxdamage1 \
        libxfixes3 \
        libxkbcommon0 \
        libxrandr2 \
        libxshmfence1 \
        fonts-liberation \
        fonts-noto-color-emoji \
        xdg-utils \
    && rustup toolchain install stable \
    && rustup default stable \
    && rustup component add rustfmt clippy \
    && rustup target add x86_64-unknown-linux-musl \
    && rm -rf /var/lib/apt/lists/*

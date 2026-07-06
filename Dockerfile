# Multi-stage build
FROM rust:1.96-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev curl

# Add target
RUN rustup target add wasm32-unknown-unknown

# Install precompiled Trunk binary for fast setup
RUN apt-get update && apt-get install -y curl tar && \
    curl -L https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz | tar -xz -C /usr/local/bin

WORKDIR /usr/src/app
COPY . .

# Build the distributable web bundle into dist/ using cargo cache mounts
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/app/target \
    trunk build --release

# Production image
FROM nginx:alpine

# Copy the Trunk bundle (hashed wasm/js + processed index.html)
COPY --from=builder /usr/src/app/dist/ /usr/share/nginx/html/

EXPOSE 80

# Multi-stage build
FROM rust:1.96-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev curl

# Add target
RUN rustup target add wasm32-unknown-unknown

# Trunk drives the whole web build (cargo build + wasm-bindgen + index.html
# asset injection), matching local `trunk serve`/`trunk build` exactly.
RUN cargo install trunk --locked

WORKDIR /usr/src/app
COPY . .

# Build the distributable web bundle into dist/
RUN trunk build --release

# Production image
FROM nginx:alpine

# Copy the Trunk bundle (hashed wasm/js + processed index.html)
COPY --from=builder /usr/src/app/dist/ /usr/share/nginx/html/

EXPOSE 80

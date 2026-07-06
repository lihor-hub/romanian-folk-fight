# Multi-stage build
FROM rust:1.96-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev curl

# Add target
RUN rustup target add wasm32-unknown-unknown

# Install matching wasm-bindgen-cli version
RUN cargo install wasm-bindgen-cli --version 0.2.126

WORKDIR /usr/src/app
COPY . .

# Build the WASM binary
RUN cargo build --release --target wasm32-unknown-unknown

# Generate bindgen files
RUN wasm-bindgen --target web --out-dir ./out --out-name client ./target/wasm32-unknown-unknown/release/romanian-folk-fight.wasm

# Production image
FROM nginx:alpine

# Copy static assets and wasm files
COPY --from=builder /usr/src/app/out/ /usr/share/nginx/html/
COPY index.html /usr/share/nginx/html/

EXPOSE 80

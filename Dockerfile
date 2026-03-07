# Blufio Multi-Stage Dockerfile
#
# Produces a minimal container image using cargo-chef for dependency caching
# and distroless cc-debian12 as the runtime base.
#
# Build:  docker build -t blufio:latest .
# Run:    docker run -p 3000:3000 -v blufio-data:/data blufio:latest

# ── Stage 1: Plan dependencies ──────────────────────────────
FROM rust:1.85-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 2: Build dependencies + binary ────────────────────
FROM rust:1.85-bookworm AS builder

# Install build dependencies for SQLCipher (vendored OpenSSL) and bindgen.
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libclang-dev cmake make \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef --locked
WORKDIR /src

# Cook dependencies (cached layer — only invalidated when Cargo.toml/Cargo.lock change).
COPY --from=chef /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copy full source and build with all features.
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release --all-features

# Collect ONNX Runtime shared libraries deposited by ort's copy-dylibs feature.
RUN mkdir -p /ort-libs && \
    find target/release -maxdepth 1 -name 'libonnxruntime*' -exec cp {} /ort-libs/ \; || true

# ── Stage 3: Minimal runtime image ─────────────────────────
# Using cc-debian12 (not static-debian12) because ONNX Runtime ships
# glibc-linked shared libraries (.so files) that need libc at runtime.
FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder /src/target/release/blufio /blufio
COPY --from=builder /ort-libs/ /usr/lib/

ENV BLUFIO_CONFIG=/config/config.toml
ENV RUST_LOG=blufio=info

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/blufio", "healthcheck"]

ENTRYPOINT ["/blufio"]
CMD ["serve"]

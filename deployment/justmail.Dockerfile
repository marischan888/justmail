FROM lukemathwalker/cargo-chef:0.1.77-rust-1.96.0-trixie AS chef
WORKDIR /app
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

FROM chef AS planner
COPY . .
# Compute a lock-like file for our project
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release --bin justmail
# Install sqlx-cli
RUN wget --progress=dot:giga https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-musl.tgz \
  && tar -xvf cargo-binstall-x86_64-unknown-linux-musl.tgz \
  && cp cargo-binstall /usr/local/cargo/bin \
  && rm cargo-binstall-x86_64-unknown-linux-musl.tgz
RUN cargo binstall -y sqlx-cli \
  && rm -rf /usr/local/cargo/registry/cache/* \
  && rm -rf /usr/local/cargo/registry/src/*

# We do not need the Rust toolchain to run the binary!
FROM debian:trixie-slim AS runtime
WORKDIR /app
RUN apt-get update -y \
  && apt-get install -y --no-install-recommends openssl ca-certificates curl \
  && apt-get autoremove -y \
  && apt-get clean -y \
  && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/justmail justmail
COPY --from=builder /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx

COPY configuration configuration
COPY migrations migrations

ENV APP_ENVIRONMENT=production
ENTRYPOINT ["./justmail"]

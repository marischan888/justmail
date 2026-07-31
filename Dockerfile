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

# Rumetime stage
FROM debian:trixie-slim AS runtime
WORKDIR /app
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*
# copy the compiled binary from the builder to runtime env
COPY --from=builder /app/target/release/justmail justmail
# copy config files to runtime
COPY configuration configuration
ENV APP_ENVIRONMENT=production
ENTRYPOINT ["./justmail"]

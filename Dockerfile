# Builder stage
FROM rust:1.93.1-slim AS builder
WORKDIR /app
RUN apt update && apt install lld clang -y
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release

# Rumetime stage
FROM debian:trixie-slim AS runtime
WORKDIR /app
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    ## clean up \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*
# copy the compiled binary from the builder to runtime env
COPY --from=builder /app/target/release/justmail justmail
# copy config files to runtime
COPY configuration configuration
ENV APP_ENVIRONMENT=production
ENTRYPOINT ["./justmail"]
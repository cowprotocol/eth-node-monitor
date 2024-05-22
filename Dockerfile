FROM docker.io/lukemathwalker/cargo-chef:0.1.67-rust-bookworm AS chef
WORKDIR /app

LABEL org.opencontainers.image.source=https://github.com/cowprotocol/eth-node-monitor
LABEL org.opencontainers.image.licenses="GPL-3.0"

# Builds a cargo-chef plan
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build profile, release by default
ARG BUILD_PROFILE=release
ENV BUILD_PROFILE $BUILD_PROFILE

# Extra Cargo flags
ARG RUSTFLAGS=""
ENV RUSTFLAGS "$RUSTFLAGS"

# Extra Cargo features
ARG FEATURES=""
ENV FEATURES $FEATURES

# Install system dependencies
RUN apt-get update && apt-get -y upgrade && apt-get install -y libclang-dev libssl-dev pkg-config

# Builds dependencies
RUN cargo chef cook --profile $BUILD_PROFILE --features "$FEATURES" --recipe-path recipe.json

# Build application
COPY . .
RUN cargo build --profile $BUILD_PROFILE --features "$FEATURES" --locked

# ARG is not resolved in COPY so we have to hack around it by copying the
# binary to a temporary location
RUN cp /app/target/$BUILD_PROFILE/eth-node-monitor /app/eth-node-monitor

# Use Debian as the release image
FROM docker.io/library/debian:bookworm-slim AS runtime
WORKDIR /app

# Copy eth-node-monitor over from the build stage
COPY --from=builder /app/eth-node-monitor /usr/local/bin

# Copy licenses
COPY LICENSE-* ./

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
  && apt-get -y install libssl3 \
  && apt-get clean \
  && rm -rf /var/lib/apt/lists/*

EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/eth-node-monitor"]

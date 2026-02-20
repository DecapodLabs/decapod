# Default Dockerfile template for Decapod agent environments
# This template is exploded to .decapod/generated/Dockerfile during project initialization
# Edit this file to customize the base image and dependencies

FROM ubuntu:22.04

# Install common dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Default tool versions (can be overridden)
ARG RUST_VERSION=1.85.0
ARG NODE_VERSION=20

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain ${RUST_VERSION}
ENV PATH="/root/.cargo/bin:${PATH}"

# Install Node.js
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION}.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# Default working directory
WORKDIR /workspace

# Default user
USER root

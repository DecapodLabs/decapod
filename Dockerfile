# Decapod Workspace Dockerfile
# Auto-generated for reproducible agent environments

FROM rust:1.75-slim

# Install essential tools
RUN apt-get update && apt-get install -y \
    git \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install decapod
RUN cargo install decapod

# Set up workspace
WORKDIR /workspace
ENV DECAPOD_IN_CONTAINER=true
ENV DECAPOD_WORKSPACE_IMAGE=decapod-workspace

# Default command
CMD ["/bin/bash"]

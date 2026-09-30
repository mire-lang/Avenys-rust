FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# Install dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    build-essential \
    clang \
    llvm \
    lld \
    libssl-dev \
    pkg-config \
    libsodium-dev \
    zstd \
    libzstd-dev \
    libarchive-dev \
    zlib1g-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install wasmtime for WASM testing
RUN curl https://wasmtime.dev/install.sh -sSf | bash -s -- -y
ENV PATH="/root/.wasmtime/bin:${PATH}"

# Set up WASI SDK
ENV WASI_SDK_PATH=/opt/wasi-sdk
RUN mkdir -p /opt/wasi-sdk && \
    curl -L https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-27/wasi-sdk-27.0-linux.tar.gz | tar -xz -C /opt/wasi-sdk --strip-components=1

# Set up working directory
WORKDIR /workspace

# Copy source code
COPY . /workspace/avenys

# Build and install avenys
WORKDIR /workspace/avenys
RUN cargo build --release --features ffi

# Install mire (as owl command)
RUN cp target/release/mire /usr/local/bin/mire && \
    mkdir -p /usr/local/bin && \
    ln -sf /workspace/avenys/target/release/mire /usr/local/bin/owl

# Install mire stdlib
ENV MIRE_LIB_DIR=/usr/local/lib/mire
RUN mkdir -p $MIRE_LIB_DIR

# Set up environment
ENV PATH="/root/.cargo/bin:/root/.wasmtime/bin:${PATH}"
ENV WASI_SDK_PATH=/opt/wasi-sdk

# Set up owl config
RUN mkdir -p /root/.owl
RUN echo '[tool.owl.auto-deps]
mire = { path = "/usr/local/lib/mire", version = "0.0.1" }
kioto = { path = "/usr/local/lib/kioto", version = "2.4.7" }
' > /root/.owl/config.toml

# Set up working directory for tests
WORKDIR /workspace/test_projects

CMD ["/bin/bash"]

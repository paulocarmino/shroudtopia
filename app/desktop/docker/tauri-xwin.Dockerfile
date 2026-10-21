# Toolchain to cross-compile the Tauri app (Rust + WebView) from Linux -> Windows
# using cargo-xwin, per the official Tauri v2 docs.
#
# The image only PREPARES the toolchain; the build runs at runtime with the code
# mounted at /work (see scripts/build-tauri.sh). This way nothing is installed on
# your Windows.
FROM rust:latest

# System dependencies required by the Tauri cross-compile (official docs:
# `apt install nsis lld llvm clang`) + curl/ca-certificates for Node.
RUN apt-get update && apt-get install -y --no-install-recommends \
        nsis \
        lld \
        llvm \
        clang \
        ninja-build \
        curl \
        ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# Node LTS (Svelte/Vite frontend build). setup_lts.x always brings the current
# LTS — no hardcoded version.
RUN curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && rm -rf /var/lib/apt/lists/*

# Expensive steps first (compile for several minutes) to maximize Docker caching:
# they stay intact when only the toolchain config below changes.
RUN cargo install --locked cargo-xwin \
 && cargo install --locked tauri-cli

# Toolchain config on the `stable` channel — the SAME one the project's
# rust-toolchain.toml selects at runtime. This is where the Windows target must
# exist.
RUN rustup default stable \
 && rustup target add x86_64-pc-windows-msvc \
 && rustup component add llvm-tools

WORKDIR /work

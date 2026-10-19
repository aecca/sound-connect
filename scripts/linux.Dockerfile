# Builds, tests and lints the workspace for Linux, e.g. from a Mac:
#
#   docker build -t sound-connect-linux -f scripts/linux.Dockerfile scripts
#   docker run --rm -v "$PWD":/src -v sound-connect-cargo:/usr/local/cargo/registry \
#     -v sound-connect-target:/target sound-connect-linux
#
# The volumes keep downloaded crates and build output between runs. This only
# compiles and runs the tests: there's no Bluetooth or desktop in here.
FROM rust:1.99-trixie

COPY linux-build-deps.sh /tmp/
RUN /tmp/linux-build-deps.sh && rm -rf /var/lib/apt/lists/*
RUN rustup component add clippy

# Docker Desktop's default 8 GB runs out compiling GPUI with a job per core.
ENV CARGO_TARGET_DIR=/target CARGO_BUILD_JOBS=4 CARGO_PROFILE_DEV_DEBUG=0
WORKDIR /src
CMD ["sh", "-c", "cargo build --workspace && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings"]

# Ubuntu 22.04 build environment for Linux releases; see scripts/package-linux.sh.
FROM ubuntu:22.04
COPY linux-build-deps.sh /tmp/
RUN /tmp/linux-build-deps.sh && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal

# Docker Desktop's default memory runs out compiling GPUI with a job per core.
ENV PATH=/root/.cargo/bin:$PATH CARGO_TARGET_DIR=/target CARGO_BUILD_JOBS=4
WORKDIR /src
CMD ["scripts/package-linux.sh"]

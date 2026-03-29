FROM rust:1-bookworm

ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV PATH=/usr/local/cargo/bin:${PATH}

RUN apt-get update && apt-get install -y --no-install-recommends \
    at-spi2-core \
    build-essential \
    ca-certificates \
    curl \
    dpkg \
    git \
    libayatana-appindicator3-dev \
    libgtk-3-dev \
    libgl1-mesa-dev \
    librsvg2-dev \
    libudev-dev \
    libwebkit2gtk-4.1-dev \
    libx11-dev \
    libxcursor-dev \
    libxi-dev \
    libxinerama-dev \
    libxrandr-dev \
    mingw-w64 \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add rustfmt clippy
RUN rustup target add x86_64-pc-windows-gnu

WORKDIR /workspace
CMD ["bash"]

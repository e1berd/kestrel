FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential ca-certificates clang cmake curl desktop-file-utils dpkg-dev \
    file libfontconfig1-dev libfreetype6-dev libssl-dev libudev-dev libvulkan-dev \
    libwayland-dev libxcb1-dev libxkbcommon-x11-dev patchelf pkg-config python3 \
    squashfs-tools zsync && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup.sh && \
    sh /tmp/rustup.sh -y --profile minimal --default-toolchain stable && rm /tmp/rustup.sh
ENV PATH=/opt/cargo/bin:$PATH
WORKDIR /source
ENTRYPOINT ["bash", "packaging/linux/build.sh"]

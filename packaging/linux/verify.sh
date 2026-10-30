#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../common.sh"
require docker
version="$(package_version)"
[[ -f "$output/kestrel_${version}_amd64.deb" ]] || fail "Build the deb package first"
[[ -f "$output/Kestrel-${version}-x86_64.AppImage" ]] || fail "Build the AppImage first"
for image in ubuntu:22.04 debian:12-slim; do
    docker run --rm \
        --mount "type=bind,source=$output,target=/packages,readonly" \
        --mount "type=bind,source=$root/examples/httpbin,target=/collection,readonly" \
        "$image" bash -euc '
            export DEBIAN_FRONTEND=noninteractive
            apt-get update -qq
            apt-get install -y --no-install-recommends "/packages/kestrel_${1}_amd64.deb"
            kestrelcli --version
            kestrelcli envs /collection
            kestrel-mcp --version
            ldd /usr/bin/kestrel > /tmp/libraries
            if grep "not found" /tmp/libraries; then exit 1; fi
            cd /tmp
            "/packages/Kestrel-${1}-x86_64.AppImage" --appimage-extract > /dev/null
            ./squashfs-root/AppRun --kestrelcli envs /collection
            ./squashfs-root/AppRun --kestrel-mcp --version
            LD_LIBRARY_PATH=/tmp/squashfs-root/usr/lib ldd ./squashfs-root/usr/bin/kestrel > /tmp/libraries
            if grep "not found" /tmp/libraries; then exit 1; fi
        ' bash "$version"
done
docker run --rm \
    --mount "type=bind,source=$output,target=/packages,readonly" \
    --mount "type=bind,source=$root/examples/httpbin,target=/collection,readonly" \
    archlinux:base bash -euc '
        pacman -Syu --noconfirm --needed ca-certificates gcc-libs libxcb libxkbcommon libxkbcommon-x11 wayland vulkan-icd-loader
        cd /tmp
        "/packages/Kestrel-${1}-x86_64.AppImage" --appimage-extract > /dev/null
        ./squashfs-root/AppRun --kestrelcli envs /collection
        ./squashfs-root/AppRun --kestrel-mcp --version
        LD_LIBRARY_PATH=/tmp/squashfs-root/usr/lib ldd ./squashfs-root/usr/bin/kestrel > /tmp/libraries
        if grep "not found" /tmp/libraries; then exit 1; fi
    ' bash "$version"

#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../common.sh"
[[ "$(uname -s)" == Darwin ]] || fail "Run packaging/macos/vm.sh from Linux"
require cargo
require rustup
architecture="${KESTREL_MACOS_ARCH:-all}"
case "$architecture" in
    all)
        targets=(aarch64-apple-darwin x86_64-apple-darwin)
        packages=(aarch64 x86_64 universal)
        ;;
    universal)
        targets=(aarch64-apple-darwin x86_64-apple-darwin)
        packages=(universal)
        ;;
    aarch64|x86_64)
        targets=("$architecture-apple-darwin")
        packages=("$architecture")
        ;;
    *) fail "KESTREL_MACOS_ARCH must be all, aarch64, x86_64 or universal" ;;
esac
export MACOSX_DEPLOYMENT_TARGET=11.0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
for target in "${targets[@]}"; do
    rustup target add "$target"
    build_binaries --target "$target"
done
for architecture in "${packages[@]}"; do
    KESTREL_MACOS_ARCH="$architecture" bash "$root/packaging/macos/bundle.sh"
done

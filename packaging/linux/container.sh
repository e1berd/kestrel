#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../common.sh"
require docker
[[ "$(uname -m)" == x86_64 ]] || fail "The Linux release builder currently supports x86_64 hosts"
image="${KESTREL_LINUX_IMAGE:-kestrel-packaging:ubuntu22.04}"
cache="$root/target/packaging"
mkdir -p "$cache/cargo" "$cache/linux"
docker build --tag "$image" --file "$root/packaging/linux/Dockerfile" "$root/packaging/linux"
docker run --rm --user "$(id -u):$(id -g)" \
    --env HOME=/tmp --env CARGO_HOME=/cache/cargo --env CARGO_TARGET_DIR=/cache/linux \
    --env CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" --env KESTREL_DIST_DIR=/dist \
    --mount "type=bind,source=$root,target=/source,readonly" \
    --mount "type=bind,source=$cache,target=/cache" \
    --mount "type=bind,source=$output,target=/dist" \
    "$image" "$@"

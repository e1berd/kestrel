#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"
mkdir -p "$CARGO_TARGET_DIR"
CARGO_TARGET_DIR="$(cd "$CARGO_TARGET_DIR" && pwd)"
output="${KESTREL_DIST_DIR:-$root/dist}"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
cd "$root"

fail() {
    echo "$*" >&2
    exit 1
}

require() {
    command -v "$1" >/dev/null || fail "Required command not found: $1"
}

package_version() {
    cargo metadata --manifest-path "$root/Cargo.toml" --no-deps --locked --format-version 1 |
        python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "kestrel-app"))'
}

build_binaries() {
    cargo build --manifest-path "$root/Cargo.toml" --locked --release \
        -p kestrel-app -p kestrel-cli -p kestrel-mcp "$@"
}

download_verified() {
    local url="$1" destination="$2" expected="$3" actual
    if [[ ! -f "$destination" ]]; then
        curl --fail --silent --show-error --location --connect-timeout 20 --max-time 300 \
            --retry 3 "$url" --output "$destination.download"
        mv "$destination.download" "$destination"
    fi
    actual="$(shasum -a 256 "$destination" | cut -d ' ' -f 1)"
    [[ "$actual" == "$expected" ]] || fail "SHA256 mismatch: $destination"
}

write_checksums() {
    python3 - "$@" <<'PY'
import hashlib
from pathlib import Path
import sys

for name in sys.argv[1:]:
    artifact = Path(name)
    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
    artifact.with_name(artifact.name + '.sha256').write_text(f'{digest}  {artifact.name}\n')
PY
}

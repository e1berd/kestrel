#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../common.sh"
for command in ssh scp tar; do
    require "$command"
done
machine="${KESTREL_MACOS_VM_DIR:-$HOME/vms/macos/OSX-KVM}"
destination="${KESTREL_MACOS_SSH:-zift@127.0.0.1}"
port="${KESTREL_MACOS_SSH_PORT:-2223}"
architecture="${KESTREL_MACOS_ARCH:-all}"
case "$architecture" in all|universal|x86_64|aarch64) ;; *) fail "Invalid macOS architecture: $architecture" ;; esac
connection=(-o BatchMode=yes -o StrictHostKeyChecking=accept-new -o ConnectTimeout=8)
on_machine() {
    ssh "${connection[@]}" -p "$port" "$destination" "$@"
}

if ! on_machine true; then
    [[ -x "$machine/start.sh" ]] || fail "macOS SSH is unavailable; set KESTREL_MACOS_SSH and KESTREL_MACOS_SSH_PORT"
    if [[ -f "$machine/qemu.pid" ]] && kill -0 "$(cat "$machine/qemu.pid")" 2>/dev/null; then
        echo "macOS VM is running; waiting for SSH" >&2
    else
        (cd "$machine" && ./start.sh)
    fi
    ready=false
    for attempt in {1..30}; do
        echo "Waiting for macOS SSH ($attempt/30)" >&2
        if on_machine true; then
            ready=true
            break
        fi
        sleep 4
    done
    [[ "$ready" == true ]] || fail "macOS did not become reachable; finish booting it in its VNC console"
fi

remote="$(on_machine 'mktemp -d /tmp/kestrel-build.XXXXXX')"
[[ "$remote" == /tmp/kestrel-build.* && "$remote" != *[!a-zA-Z0-9/._-]* ]] || fail "Unexpected remote build directory"
cleanup() {
    on_machine "rm -rf '$remote'" || echo "Could not remove remote staging directory: $remote" >&2
}
trap cleanup EXIT
tar -C "$root" --exclude=.env --exclude=target --exclude=__pycache__ -czf - \
    Cargo.toml Cargo.lock rust-toolchain.toml .cargo crates packaging LICENSE-MIT LICENSE-APACHE |
    on_machine "LC_ALL=C tar -xzf - -C '$remote'"
printf -v build_command 'bash -s -- %q %q %q %q %q' \
    "$remote" "$architecture" "${CARGO_BUILD_JOBS:-2}" \
    "${KESTREL_CODESIGN_IDENTITY:--}" "${KESTREL_NOTARY_PROFILE:-}"
on_machine "$build_command" <<'REMOTE'
set -euo pipefail
source "$HOME/.cargo/env"
export CARGO_TARGET_DIR="$HOME/kestrel-build/target"
export KESTREL_DIST_DIR="$1/dist"
export KESTREL_MACOS_ARCH="$2" CARGO_BUILD_JOBS="$3"
export KESTREL_CODESIGN_IDENTITY="$4" KESTREL_NOTARY_PROFILE="$5"
mkdir -p "$CARGO_TARGET_DIR"
bash "$1/packaging/macos/build.sh"
REMOTE
scp "${connection[@]}" -P "$port" "$destination:$remote/dist/*.dmg" "$output/"
scp "${connection[@]}" -P "$port" "$destination:$remote/dist/*.sha256" "$output/"

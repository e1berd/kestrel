#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
bin="$HOME/.local/bin"

cargo build --release --manifest-path "$root/Cargo.toml" -p volley-app
install -Dm755 "$root/target/release/kestrel" "$bin/kestrel"

for size in 16 24 32 48 64 128 256 512; do
    install -Dm644 "$root/packaging/linux/icons/kestrel-$size.png" "$data/icons/hicolor/${size}x${size}/apps/dev.kestrel.app.png"
done
install -Dm644 "$root/crates/volley-app/assets/brand/kestrel.svg" "$data/icons/hicolor/scalable/apps/dev.kestrel.app.svg"

sed "s|^Exec=kestrel|Exec=$bin/kestrel|" "$root/packaging/linux/dev.kestrel.app.desktop" > "$data/applications/dev.kestrel.app.desktop"

gtk-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true
update-desktop-database -q "$data/applications" 2>/dev/null || true
echo "Installed Kestrel to $bin/kestrel"

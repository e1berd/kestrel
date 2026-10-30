#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../common.sh"
[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || fail "Expected x86_64 Linux"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
format="${1:-all}"
case "$format" in all|deb|appimage) ;; *) fail "Usage: $0 [all|deb|appimage]" ;; esac
for command in cargo python3 dpkg-shlibdeps dpkg-deb desktop-file-validate; do
    require "$command"
done
desktop-file-validate "$root/packaging/linux/dev.kestrel.app.desktop"
build_binaries --bins
version="$(package_version)"
staging="$(mktemp -d "$CARGO_TARGET_DIR/kestrel-package.XXXXXX")"
trap 'rm -rf "$staging"' EXIT
python3 "$root/packaging/linux/package.py" "$root" "$CARGO_TARGET_DIR/release" "$staging" "$output" "$version" "$format"

if [[ "$format" == all || "$format" == appimage ]]; then
    require curl
    tools="$CARGO_TARGET_DIR/packaging-tools"
    mkdir -p "$tools"
    download_verified \
        https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
        "$tools/appimagetool-1.9.1.AppImage" \
        ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
    download_verified \
        https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
        "$tools/runtime-20251108-x86_64" \
        2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
    chmod +x "$tools/appimagetool-1.9.1.AppImage"
    artifact="$output/Kestrel-${version}-x86_64.AppImage"
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 VERSION="$version" \
        "$tools/appimagetool-1.9.1.AppImage" --runtime-file "$tools/runtime-20251108-x86_64" \
        "$staging/Kestrel.AppDir" "$artifact"
    "$artifact" --appimage-version
    write_checksums "$artifact"
fi
if [[ "$format" == all || "$format" == deb ]]; then
    write_checksums "$output/kestrel_${version}_amd64.deb"
fi

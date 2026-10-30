#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../common.sh"
[[ "$(uname -s)" == Darwin ]] || fail "Run packaging/macos/vm.sh from Linux"
for command in cargo python3 xcrun sips iconutil hdiutil codesign lipo plutil; do
    require "$command"
done
architecture="${KESTREL_MACOS_ARCH:-aarch64}"
case "$architecture" in
    universal) targets=(x86_64-apple-darwin aarch64-apple-darwin) ;;
    x86_64) targets=(x86_64-apple-darwin) ;;
    aarch64) targets=(aarch64-apple-darwin) ;;
    *) fail "KESTREL_MACOS_ARCH must be universal, x86_64 or aarch64" ;;
esac
export MACOSX_DEPLOYMENT_TARGET=11.0
version="$(package_version)"
staging="$(mktemp -d "$CARGO_TARGET_DIR/kestrel-macos.XXXXXX")"
trap 'rm -rf "$staging"' EXIT
application="$staging/image/Kestrel.app"
mkdir -p "$application/Contents/MacOS" "$application/Contents/Resources/licenses" "$staging/Kestrel.iconset"
for executable in kestrel kestrelcli kestrel-mcp; do
    inputs=()
    for target in "${targets[@]}"; do
        inputs+=("$CARGO_TARGET_DIR/$target/release/$executable")
    done
    lipo -create "${inputs[@]}" -output "$application/Contents/MacOS/$executable"
    chmod +x "$application/Contents/MacOS/$executable"
    lipo -info "$application/Contents/MacOS/$executable"
done
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$root/packaging/linux/icons/kestrel-512.png" \
        --out "$staging/Kestrel.iconset/icon_${size}x${size}.png" >/dev/null
    doubled=$((size * 2))
    sips -z "$doubled" "$doubled" "$root/packaging/linux/icons/kestrel-512.png" \
        --out "$staging/Kestrel.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$staging/Kestrel.iconset" -o "$application/Contents/Resources/Kestrel.icns"
python3 - "$application" "$version" "$MACOSX_DEPLOYMENT_TARGET" <<'PY'
import plistlib
from pathlib import Path
import sys

application, version, minimum = sys.argv[1:]
properties = dict(CFBundleDevelopmentRegion='en', CFBundleDisplayName='Kestrel',
                  CFBundleExecutable='kestrel', CFBundleIconFile='Kestrel.icns',
                  CFBundleIdentifier='dev.kestrel.app', CFBundleName='Kestrel',
                  CFBundlePackageType='APPL', CFBundleShortVersionString=version,
                  CFBundleVersion=version, LSMinimumSystemVersion=minimum,
                  LSApplicationCategoryType='public.app-category.developer-tools',
                  NSHighResolutionCapable=True, NSSupportsAutomaticGraphicsSwitching=True,
                  NSPrincipalClass='NSApplication')
Path(application, 'Contents/Info.plist').write_bytes(plistlib.dumps(properties))
PY
cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" \
    "$root/crates/kestrel-app/assets/fonts/Inter-LICENSE.txt" \
    "$root/crates/kestrel-app/assets/fonts/JetBrainsMono-LICENSE.txt" \
    "$application/Contents/Resources/licenses/"
identity="${KESTREL_CODESIGN_IDENTITY:--}"
signing=(--force --sign "$identity")
if [[ "$identity" != - ]]; then
    signing+=(--timestamp --options runtime)
fi
for executable in kestrelcli kestrel-mcp kestrel; do
    codesign "${signing[@]}" "$application/Contents/MacOS/$executable"
done
codesign "${signing[@]}" "$application"
codesign --verify --deep --strict --verbose=2 "$application"
plutil -lint "$application/Contents/Info.plist"
if [[ "$architecture" == universal || "$architecture" == "$(uname -m)" ||
    ( "$architecture" == aarch64 && "$(uname -m)" == arm64 ) ]]; then
    "$application/Contents/MacOS/kestrelcli" --help >/dev/null
fi
ln -s /Applications "$staging/image/Applications"
artifact="$output/Kestrel-${version}-${architecture}.dmg"
hdiutil create -volname Kestrel -srcfolder "$staging/image" -format UDZO -ov "$artifact"
hdiutil verify "$artifact"
if [[ "$identity" != - ]]; then
    codesign --force --timestamp --sign "$identity" "$artifact"
fi
if [[ -n "${KESTREL_NOTARY_PROFILE:-}" ]]; then
    [[ "$identity" != - ]] || fail "Notarization requires KESTREL_CODESIGN_IDENTITY"
    xcrun notarytool submit "$artifact" --keychain-profile "$KESTREL_NOTARY_PROFILE" --wait
    xcrun stapler staple "$artifact"
    xcrun stapler validate "$artifact"
fi
write_checksums "$artifact"

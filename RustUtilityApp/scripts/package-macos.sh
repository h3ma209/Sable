#!/usr/bin/env bash
# Build "Sable.app" plus a disk image and a zip for GitHub Releases.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' crates/ui/Cargo.toml | head -1)"
if [[ -z "$version" ]]; then
  echo "Could not read version from crates/ui/Cargo.toml" >&2
  exit 1
fi

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"

echo "Building Sable ${version} for ${MACOSX_DEPLOYMENT_TARGET}+"
cargo build -p sable --release

app="$root/dist/Sable.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$root/target/release/sable" "$app/Contents/MacOS/Sable"
chmod +x "$app/Contents/MacOS/Sable"
printf 'APPL????' > "$app/Contents/PkgInfo"

iconset="$(mktemp -d)/Sable.iconset"
icon_python="python3"
if ! python3 -c "import PIL" 2>/dev/null; then
  venv="$root/.venv-packaging"
  python3 -m venv "$venv"
  "$venv/bin/pip" install -q pillow numpy
  icon_python="$venv/bin/python3"
fi
"$icon_python" "$root/scripts/make_icon.py" "$iconset"
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Sable.icns"
rm -rf "$(dirname "$iconset")"

# Copy UI assets (CSS, icons, fonts) into app bundle resources
cp -R "$root/crates/ui/assets" "$app/Contents/Resources/assets"

sed "s/VERSION/${version}/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist" >/dev/null

# Code signing: developer identity or fallback to ad-hoc.
SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:--}"
echo "Signing with identity: ${SIGNING_IDENTITY}"
codesign --force --options runtime --sign "${SIGNING_IDENTITY}" "$app"
codesign --verify --verbose=2 "$app"

# Optional Apple notarization if NOTARIZE_PROFILE is configured
if [[ -n "${NOTARIZE_PROFILE:-}" && "${SIGNING_IDENTITY}" != "-" ]]; then
  echo "Submitting to Apple notary service using profile: ${NOTARIZE_PROFILE}"
  xcrun notarytool submit "$app" --keychain-profile "${NOTARIZE_PROFILE}" --wait
  xcrun stapler staple "$app"
fi

arch_name="$(uname -m)"
case "$arch_name" in
  arm64) arch_label="arm64" ;;
  x86_64) arch_label="x64" ;;
  *) arch_label="$arch_name" ;;
esac

stage="$(mktemp -d)"
cp -R "$app" "$stage/Sable.app"
ln -s /Applications "$stage/Applications"
dmg="$root/dist/Sable-${version}-macos-${arch_label}.dmg"
hdiutil create -volname "Sable" -srcfolder "$stage" -ov -format UDZO "$dmg"
rm -rf "$stage"

zip="$root/dist/Sable-${version}-macos-${arch_label}.zip"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"

echo
echo "Installable builds:"
ls -lh "$app" "$dmg" "$zip"

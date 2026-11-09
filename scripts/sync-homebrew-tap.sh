#!/usr/bin/env bash
# Refresh packaging/homebrew/Casks/sable.rb from a GitHub release SHA256SUMS.txt
set -euo pipefail

REPO="${SABLE_REPO:-h3ma209/Sable}"
TAG="${1:?Usage: sync-homebrew-tap.sh v0.3.10 [path-to-homebrew-sable-clone]}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SUMS_URL="https://github.com/${REPO}/releases/download/${TAG}/SHA256SUMS.txt"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

curl -fsSL "$SUMS_URL" -o "$TMP"
VERSION="${TAG#v}"

sha_arm_dmg="$(awk '/macos-arm64\.dmg$/ {print $1; exit}' "$TMP")"
sha_x64_dmg="$(awk '/macos-x64\.dmg$/ {print $1; exit}' "$TMP")"

if [[ -z "${sha_arm_dmg:-}" ]]; then
  echo "No arm64 DMG line in checksum file" >&2
  exit 1
fi

CASK_DIR="${2:-$ROOT/../homebrew-sable}"
CASK="$CASK_DIR/Casks/sable.rb"
mkdir -p "$(dirname "$CASK")"

if [[ -n "${sha_x64_dmg:-}" ]]; then
  cat > "$CASK" <<RUBY
cask "sable" do
  version "${VERSION}"

  on_arm do
    sha256 "${sha_arm_dmg}"
    url "https://github.com/${REPO}/releases/download/${TAG}/Sable-${VERSION}-macos-arm64.dmg"
  end
  on_intel do
    sha256 "${sha_x64_dmg}"
    url "https://github.com/${REPO}/releases/download/${TAG}/Sable-${VERSION}-macos-x64.dmg"
  end

  name "Sable"
  desc "Open-source disk cleaner and space analyzer for macOS"
  homepage "https://github.com/${REPO}"

  app "Sable.app"

  zap trash: [
    "~/Library/Caches/dev.sable",
    "~/Library/Preferences/dev.sable.plist",
  ]
end
RUBY
else
  cat > "$CASK" <<RUBY
cask "sable" do
  version "${VERSION}"
  sha256 "${sha_arm_dmg}"
  url "https://github.com/${REPO}/releases/download/${TAG}/Sable-${VERSION}-macos-arm64.dmg"

  name "Sable"
  desc "Open-source disk cleaner and space analyzer for macOS"
  homepage "https://github.com/${REPO}"
  depends_on arch: :arm64

  app "Sable.app"

  zap trash: [
    "~/Library/Caches/dev.sable",
    "~/Library/Preferences/dev.sable.plist",
  ]
end
RUBY
fi

cp "$CASK" "$ROOT/packaging/homebrew/Casks/sable.rb"
echo "Wrote $CASK"

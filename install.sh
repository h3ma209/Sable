#!/usr/bin/env bash
# Sable One-Line Installer for macOS
set -euo pipefail

REPO="h3ma209/Sable"
APP_NAME="Sable.app"
DEST_DIR="/Applications"

echo "==> Installing Sable..."

# Check macOS
if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Error: Sable is only supported on macOS." >&2
  exit 1
fi

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
  arm64) ARCH_LABEL="arm64" ;;
  x86_64) ARCH_LABEL="x64" ;;
  *)
    echo "Error: Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

echo "==> Fetching latest release info for ${ARCH_LABEL}..."
LATEST_TAG=$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [[ -z "${LATEST_TAG:-}" ]]; then
  LATEST_TAG="v0.2.2"
fi

ZIP_NAME="Sable-${LATEST_TAG#v}-macos-${ARCH_LABEL}.zip"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${ZIP_NAME}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "==> Downloading ${ZIP_NAME}..."
curl -sSL -o "${TMP_DIR}/${ZIP_NAME}" "$DOWNLOAD_URL"

echo "==> Extracting into ${DEST_DIR}..."
ditto -x -k "${TMP_DIR}/${ZIP_NAME}" "${TMP_DIR}"

# Remove existing installed app if present
if [[ -d "${DEST_DIR}/${APP_NAME}" ]]; then
  echo "==> Replacing previous Sable installation..."
  rm -rf "${DEST_DIR}/${APP_NAME}"
fi

cp -R "${TMP_DIR}/${APP_NAME}" "${DEST_DIR}/${APP_NAME}"

# Clear macOS Quarantine attribute to bypass Gatekeeper block
echo "==> Removing macOS quarantine attribute..."
xattr -cr "${DEST_DIR}/${APP_NAME}" 2>/dev/null || true

echo "==> Successfully installed Sable to /Applications/Sable.app!"
echo "==> Launching Sable..."
open -a "${DEST_DIR}/${APP_NAME}"

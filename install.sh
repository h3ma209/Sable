#!/usr/bin/env bash
# Sable installer — downloads the official GitHub Release artifact into /Applications.
# Review this script before running. Prefer: DMG install, Homebrew cask, or build from source.
set -euo pipefail

REPO="h3ma209/Sable"
APP_NAME="Sable.app"
DEST_DIR="/Applications"

usage() {
  cat <<'EOF'
Usage:
  bash install.sh                 Download latest release to /Applications
  SABLE_CLEAR_QUARANTINE=1 ...    Also run xattr -cr (bypass Gatekeeper quarantine — advanced)
  SABLE_VERIFY=1 ...              Require SHA256SUMS.txt on the release and verify the zip

Safer first-time launch without clearing quarantine:
  Control-click Sable.app in /Applications → Open
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

echo "==> Installing Sable (GitHub Releases → /Applications)..."
echo "    Tip: read this script first, or install via DMG / Homebrew / cargo build."

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Error: Sable is only supported on macOS." >&2
  exit 1
fi

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
LATEST_TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [[ -z "${LATEST_TAG:-}" ]]; then
  echo "Error: Could not resolve latest release tag." >&2
  exit 1
fi

VERSION="${LATEST_TAG#v}"
ZIP_NAME="Sable-${VERSION}-macos-${ARCH_LABEL}.zip"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${ZIP_NAME}"
SUMS_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/SHA256SUMS.txt"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "==> Downloading ${ZIP_NAME}..."
curl -fsSL -o "${TMP_DIR}/${ZIP_NAME}" "$DOWNLOAD_URL"

if [[ "${SABLE_VERIFY:-0}" == "1" ]]; then
  echo "==> Verifying SHA-256 (required)..."
  if ! curl -fsSL -o "${TMP_DIR}/SHA256SUMS.txt" "$SUMS_URL"; then
    echo "Error: SHA256SUMS.txt not found on release ${LATEST_TAG}. Verify manually or set SABLE_VERIFY=0." >&2
    exit 1
  fi
  EXPECTED=$(grep " ${ZIP_NAME}\$" "${TMP_DIR}/SHA256SUMS.txt" | awk '{print $1}')
  if [[ -z "${EXPECTED:-}" ]]; then
    echo "Error: ${ZIP_NAME} not listed in SHA256SUMS.txt" >&2
    exit 1
  fi
  ACTUAL=$(shasum -a 256 "${TMP_DIR}/${ZIP_NAME}" | awk '{print $1}')
  if [[ "$ACTUAL" != "$EXPECTED" ]]; then
    echo "Error: SHA-256 mismatch for ${ZIP_NAME}" >&2
    exit 1
  fi
  echo "    Checksum OK."
fi

echo "==> Extracting into ${DEST_DIR}..."
ditto -x -k "${TMP_DIR}/${ZIP_NAME}" "${TMP_DIR}"

if [[ -d "${DEST_DIR}/${APP_NAME}" ]]; then
  echo "==> Replacing previous Sable installation..."
  rm -rf "${DEST_DIR}/${APP_NAME}"
fi

cp -R "${TMP_DIR}/${APP_NAME}" "${DEST_DIR}/${APP_NAME}"

if [[ "${SABLE_CLEAR_QUARANTINE:-0}" == "1" ]]; then
  echo "==> Advanced: clearing Gatekeeper quarantine (xattr -cr)..."
  xattr -cr "${DEST_DIR}/${APP_NAME}" 2>/dev/null || true
else
  echo "==> Quarantine not cleared (default)."
  echo "    First launch: Control-click Sable in /Applications → Open"
  echo "    To strip quarantine anyway: SABLE_CLEAR_QUARANTINE=1 bash install.sh"
fi

echo "==> Installed ${DEST_DIR}/${APP_NAME}"
open -a "${DEST_DIR}/${APP_NAME}" 2>/dev/null || true

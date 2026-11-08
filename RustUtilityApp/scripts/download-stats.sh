#!/usr/bin/env bash
set -euo pipefail

REPO="h3ma209/Sable"

echo "==> Fetching release download counts for ${REPO}..."

# Try gh CLI first, fallback to curl
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
    JSON=$(gh api "repos/${REPO}/releases")
else
    JSON=$(curl -sSL -H "Accept: application/vnd.github+json" "https://api.github.com/repos/${REPO}/releases")
fi

python3 - <<EOF
import sys, json

try:
    releases = json.loads('''$JSON''')
except Exception:
    # In case json is too large for shell expansion, read stdin
    releases = []

if not releases:
    import urllib.request
    req = urllib.request.Request("https://api.github.com/repos/$REPO/releases", headers={"User-Agent": "Sable-Stats"})
    with urllib.request.urlopen(req) as resp:
        releases = json.loads(resp.read().decode())

total_all = 0
print("\n" + "=" * 55)
print(f" {'TAG':<10} | {'TOTAL':<8} | {'ASSETS BREAKDOWN'}")
print("=" * 55)

for r in releases:
    tag = r.get("tag_name", "unknown")
    assets = r.get("assets", [])
    rel_total = sum(a.get("download_count", 0) for a in assets)
    total_all += rel_total
    
    breakdown = ", ".join(f"{a.get('name', '').split('-')[-1]}: {a.get('download_count', 0)}" for a in assets)
    print(f" {tag:<10} | {rel_total:<8} | {breakdown}")

print("=" * 55)
print(f"  TOTAL DOWNLOADS ACROSS ALL RELEASES: {total_all}")
print("=" * 55 + "\n")
EOF

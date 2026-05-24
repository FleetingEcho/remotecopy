#!/usr/bin/env bash
# Build RemoteCopy GUI for macOS and collect artifacts.
# Run this on a real macOS machine (not WSL, not Linux).
#
# Usage:
#   bash scripts/release-mac.sh 0.1.3
set -euo pipefail

VERSION=${1:-}
if [[ -z "$VERSION" ]]; then
    VERSION=$(grep '"version"' remotecopy-gui/src-tauri/tauri.conf.json | awk -F'"' '{print $4}')
fi

GUI_DIR="remotecopy-gui"
TAURI_DIR="$GUI_DIR/src-tauri"
OUT_DIR="release-artifacts"

echo ""
echo "=== RemoteCopy macOS build v$VERSION ==="
echo ""

# ── Check prerequisites ───────────────────────────────────────────────────────
for cmd in cargo bun; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "Error: '$cmd' not found. See README for setup instructions." >&2
        exit 1
    fi
done

# ── Bump version ──────────────────────────────────────────────────────────────
echo "[1/3] Bumping version to $VERSION..."
sed -i.bak "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" "$TAURI_DIR/tauri.conf.json" && rm "$TAURI_DIR/tauri.conf.json.bak"
echo "    Done."
echo ""

# ── Build ─────────────────────────────────────────────────────────────────────
echo "[2/3] Building GUI..."
cd "$GUI_DIR"
bun install --frozen-lockfile
bun run tauri build -- --bundles dmg,app
cd ..
echo "    Done."

# ── Collect artifacts ─────────────────────────────────────────────────────────
echo "[3/3] Collecting artifacts..."
mkdir -p "$OUT_DIR"

BUNDLE_DIR="$TAURI_DIR/target/release/bundle"

# .dmg  →  RemoteCopy-{version}-mac-{arch}.dmg
for dmg in "$BUNDLE_DIR"/dmg/*.dmg; do
    [[ -f "$dmg" ]] || continue
    base=$(basename "$dmg")
    if [[ "$base" == *aarch64* ]]; then
        arch="arm64"
    else
        arch="x64"
    fi
    dest="$OUT_DIR/RemoteCopy-$VERSION-mac-$arch.dmg"
    cp "$dmg" "$dest"
    echo "    $(basename "$dmg")  →  $(basename "$dest")"
done

echo "    Done."

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "macOS artifacts ready in: $OUT_DIR/"
ls -lh "$OUT_DIR"/RemoteCopy-*-mac-*.dmg 2>/dev/null || true
echo ""
echo "Upload to GitHub release:"
echo "  gh release upload v$VERSION $OUT_DIR/RemoteCopy-*-mac-*.dmg"
echo ""

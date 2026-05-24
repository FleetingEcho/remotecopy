#!/usr/bin/env bash
# Build both Windows GUI executables (NSIS installer + portable) in one command.
#
# ⚠️  Run this on NATIVE Windows (Git Bash / MSYS2), NOT in WSL.
#     WSL is Linux — Tauri requires the Windows build toolchain.
#
# Prerequisites:
#   - Rust toolchain (rustup)
#   - Bun (https://bun.sh)
#   - WebView2 (bundled in Windows 11 / Edge; install for Windows 10)
#
# Usage:
#   bash scripts/build-windows.sh [version]
#
# If version is omitted it is read from tauri.conf.json.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GUI_DIR="$ROOT/remotecopy-gui"
TAURI_DIR="$GUI_DIR/src-tauri"
OUT_DIR="$ROOT/release-artifacts"

echo ""
echo "=== RemoteCopy Windows build ==="
echo ""

# ── Resolve version ──────────────────────────────────────────────────────────
VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
    VERSION=$(grep '"version"' "$TAURI_DIR/tauri.conf.json" | head -1 | awk -F'"' '{print $4}')
fi
echo "Version: $VERSION"
echo ""

# ── Install frontend deps ────────────────────────────────────────────────────
echo "[1/3] Installing frontend dependencies..."
cd "$GUI_DIR"
bun install
echo "  Done."
echo ""

# ── Build GUI ────────────────────────────────────────────────────────────────
echo "[2/3] Building GUI (bun run tauri build)..."
bun run tauri build --bundles nsis
echo "  Done."
echo ""

# ── Collect artifacts ────────────────────────────────────────────────────────
echo "[3/3] Collecting artifacts..."
mkdir -p "$OUT_DIR"

# NSIS installer → RemoteCopy-Setup-{version}-win64.exe
NSIS=$(find "$TAURI_DIR/target/release/bundle/nsis" -name "*-setup.exe" 2>/dev/null | head -1)
if [[ -n "$NSIS" ]]; then
    cp "$NSIS" "$OUT_DIR/RemoteCopy-Setup-$VERSION-win64.exe"
    echo "  → $OUT_DIR/RemoteCopy-Setup-$VERSION-win64.exe"
else
    echo "  [WARN] NSIS installer not found — check Tauri build output." >&2
fi

# Portable exe → RemoteCopy-Portable-{version}-win64.exe
PORTABLE="$TAURI_DIR/target/release/remotecopy-gui.exe"
if [[ -f "$PORTABLE" ]]; then
    cp "$PORTABLE" "$OUT_DIR/RemoteCopy-Portable-$VERSION-win64.exe"
    echo "  → $OUT_DIR/RemoteCopy-Portable-$VERSION-win64.exe"
else
    echo "  [WARN] Portable exe not found at $PORTABLE" >&2
fi

echo "  Done."
echo ""
echo "=== Windows artifacts ready in: $OUT_DIR ==="
ls -lh "$OUT_DIR"
echo ""

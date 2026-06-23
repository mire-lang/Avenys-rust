#!/bin/sh
set -e

# ── Avenys (mire) Install Script ──────────────────────────────────────
# Installs the mire compiler from the latest GitHub release artifact
# Usage: curl -fsSL <url> | sh

REPO="mire-lang/Avenys-rust"
PREFIX="${MIRE_PREFIX:-/usr/local}"
BIN_DIR="${PREFIX}/bin"
LIB_DIR="${PREFIX}/lib/mire"
DEFAULT_TAG="v3.11.28"

echo "┌─ Avenys Mire Compiler Install ──────────────────────────────────┐"
echo "│ repo:   ${REPO}"
echo "│ prefix: ${PREFIX}"
echo "│ tag:    ${MIRE_TAG:-$DEFAULT_TAG}"
echo "└─────────────────────────────────────────────────────────────────┘"

TAG="${MIRE_TAG:-$DEFAULT_TAG}"
TARBALL="mire-linux-x86_64.tar.gz"
URL="https://github.com/${REPO}/releases/download/${TAG}/${TARBALL}"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo ""
echo "  downloading ${URL}..."
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$URL" -o "$TMPDIR/$TARBALL"
elif command -v wget >/dev/null 2>&1; then
    wget -q "$URL" -O "$TMPDIR/$TARBALL"
else
    echo "error: need curl or wget"
    exit 1
fi

echo "  extracting..."
tar xzf "$TMPDIR/$TARBALL" -C "$TMPDIR"

echo "  installing mire to ${BIN_DIR}..."
sudo mkdir -p "$BIN_DIR" "$LIB_DIR"
sudo cp "$TMPDIR/mire/mire" "$BIN_DIR/mire"
sudo chmod +x "$BIN_DIR/mire"

if [ -d "$TMPDIR/mire/runtime" ]; then
    sudo cp -r "$TMPDIR/mire/runtime" "$LIB_DIR/"
fi
if [ -d "$TMPDIR/mire/pal" ]; then
    sudo cp -r "$TMPDIR/mire/pal" "$LIB_DIR/"
fi
if [ -d "$TMPDIR/mire/kioto" ]; then
    echo "  installing kioto stdlib..."
    mkdir -p "$HOME/.owl/modules"
    cp -r "$TMPDIR/mire/kioto" "$HOME/.owl/modules/kioto"
fi

echo ""
echo "  verifying..."
"$BIN_DIR/mire" --version || echo "  (version check failed, continuing)"

echo ""
echo "  install complete: $(which mire || echo "$BIN_DIR/mire")"
echo "  try: mire --help"

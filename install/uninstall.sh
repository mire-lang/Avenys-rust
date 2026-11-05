#!/bin/bash
set -e

# Mire Toolchain Uninstaller
# Removes all components installed by the Mire installer

# Default installation paths (same as installer)
if [ "$(id -u)" = "0" ]; then
    DEFAULT_PREFIX="/usr/local"
    IS_ROOT=1
else
    DEFAULT_PREFIX="$HOME/.local"
    IS_ROOT=0
fi

PREFIX="${MIRE_PREFIX:-$DEFAULT_PREFIX}"
BIN_DIR="${PREFIX}/bin"
OWL_HOME="$HOME/.owl"
MIRE_HOME="$HOME/.mire"

FORCE=0
DRY_RUN=0
REMOVE_LIBS=1
REMOVE_CONFIG=1
REMOVE_PATH=1

usage() {
    cat << 'USAGE'
Mire Toolchain Uninstaller

Usage:
  uninstall.sh [options]

Options:
  --prefix <path>       Installation prefix (default: /usr/local or ~/.local)
  --force, -f           Non-interactive, don't prompt for confirmation
  --dry-run             Show what would be removed without actually removing
  --keep-libs           Keep ~/.owl/libs and ~/.mire directories
  --keep-config         Keep ~/.owl/config.toml and registry
  --keep-path           Don't remove PATH entries from shell profiles
  --help, -h            Show this help

Examples:
  # Interactive uninstall
  curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | sh
  
  # Force uninstall everything
  curl -fsSL .../uninstall.sh | sh -s -- --force
  
  # Dry run to see what would be removed
  curl -fsSL .../uninstall.sh | sh -s -- --dry-run
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix) PREFIX="$2"; BIN_DIR="${PREFIX}/bin"; shift 2 ;;
        --force|-f) FORCE=1; shift ;;
        --dry-run) DRY_RUN=1; shift ;;
        --keep-libs) REMOVE_LIBS=0; shift ;;
        --keep-config) REMOVE_CONFIG=0; shift ;;
        --keep-path) REMOVE_PATH=0; shift ;;
        --help|-h) usage; exit 0 ;;
        *) shift ;;
    esac
done

echo "=============================================="
echo "  Mire Toolchain Uninstaller"
echo "=============================================="
echo "Prefix: $PREFIX"
echo "User: $(whoami)"
echo ""

# Collect items to remove
TO_REMOVE=()

# Binaries
[ -f "$BIN_DIR/mire" ] && TO_REMOVE+=("$BIN_DIR/mire")
[ -f "$BIN_DIR/owl" ] && TO_REMOVE+=("$BIN_DIR/owl")

# Library directory
if [ "$REMOVE_LIBS" = "1" ] && [ -d "$PREFIX/lib/mire" ]; then
    TO_REMOVE+=("$PREFIX/lib/mire")
fi

# Owl home directory
if [ -d "$OWL_HOME" ]; then
    if [ "$REMOVE_LIBS" = "1" ] && [ "$REMOVE_CONFIG" = "1" ]; then
        TO_REMOVE+=("$OWL_HOME")
    else
        # Respect --keep-libs/--keep-config: drop only the parts asked for.
        if [ "$REMOVE_LIBS" = "1" ]; then
            [ -d "$OWL_HOME/libs" ] && TO_REMOVE+=("$OWL_HOME/libs")
            [ -d "$OWL_HOME/cache" ] && TO_REMOVE+=("$OWL_HOME/cache")
            [ -d "$OWL_HOME/tmp" ] && TO_REMOVE+=("$OWL_HOME/tmp")
            [ -d "$OWL_HOME/version" ] && TO_REMOVE+=("$OWL_HOME/version")
        fi
        if [ "$REMOVE_CONFIG" = "1" ]; then
            [ -f "$OWL_HOME/config.toml" ] && TO_REMOVE+=("$OWL_HOME/config.toml")
            [ -d "$OWL_HOME/reg" ] && TO_REMOVE+=("$OWL_HOME/reg")
        fi
    fi
fi

# Mire home directory
if [ "$REMOVE_LIBS" = "1" ] && [ -d "$MIRE_HOME" ]; then
    TO_REMOVE+=("$MIRE_HOME")
fi

# Shell profile PATH entries
if [ "$REMOVE_PATH" = "1" ]; then
    for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
        if [ -f "$rc" ] && grep -qF "$BIN_DIR" "$rc"; then
            TO_REMOVE+=("$rc:$BIN_DIR")
        fi
    done
fi

if [ ${#TO_REMOVE[@]} -eq 0 ]; then
    echo "Nothing to remove. Mire toolchain not found at $PREFIX"
    exit 0
fi

echo "The following will be removed:"
for item in "${TO_REMOVE[@]}"; do
    if [[ "$item" == *":"* ]]; then
        file="${item%:*}"
        pattern="${item#*:}"
        echo "  - PATH entry '$pattern' from $file"
    else
        echo "  - $item"
    fi
done
echo ""

if [ "$DRY_RUN" = "1" ]; then
    echo "Dry run complete. No changes made."
    exit 0
fi

if [ "$FORCE" != "1" ]; then
    printf "Continue? [y/N] "
    read -r confirm
    case "$confirm" in
        [yY][eE][sS]|[yY]) ;;
        *) echo "Aborted."; exit 1 ;;
    esac
fi

echo ""
echo "Removing..."

# Remove binaries and directories
for item in "${TO_REMOVE[@]}"; do
    if [[ "$item" == *":"* ]]; then
        file="${item%:*}"
        pattern="${item#*:}"
        # Remove PATH export line from shell profile. -F keeps the prefix
        # literal; `|| true` covers a profile whose only line was the PATH
        # export (grep -v then exits 1 with an empty result).
        grep -vF "$pattern" "$file" > "$file.tmp" || true
        mv "$file.tmp" "$file"
        echo "  Removed PATH entry from $file"
    elif [ -f "$item" ]; then
        rm -f "$item"
        echo "  Removed $item"
    elif [ -d "$item" ]; then
        rm -rf "$item"
        echo "  Removed directory $item"
    fi
done

# Clean up empty parent directories
if [ -d "$PREFIX/lib" ] && [ -z "$(ls -A "$PREFIX/lib" 2>/dev/null)" ]; then
    rmdir "$PREFIX/lib" 2>/dev/null && echo "  Removed empty $PREFIX/lib"
fi

if [ -d "$BIN_DIR" ] && [ -z "$(ls -A "$BIN_DIR" 2>/dev/null)" ]; then
    rmdir "$BIN_DIR" 2>/dev/null && echo "  Removed empty $BIN_DIR"
fi

if [ "$IS_ROOT" = "1" ] && [ -d "$PREFIX" ] && [ -z "$(ls -A "$PREFIX" 2>/dev/null)" ]; then
    rmdir "$PREFIX" 2>/dev/null && echo "  Removed empty $PREFIX"
fi

echo ""
echo "=============================================="
echo "  Uninstallation complete!"
echo "=============================================="
echo ""
echo "Note: You may need to restart your shell for PATH changes to take effect."
echo ""
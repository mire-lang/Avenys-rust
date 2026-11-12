#!/bin/bash
set -eu

# Mire Toolchain Uninstaller v4.4.42
# Removes all components installed by the Mire installer

# ============================================================
# SEPARATE UNINSTALLATION LINKS (copy/paste each as needed)
# ============================================================
#
# FULL UNINSTALL (interactive):
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash
#
# FORCE UNINSTALL (non-interactive):
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --force
#
# DRY RUN (show what would be removed):
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --dry-run
#
# KEEP LIBS (remove binaries only):
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --keep-libs
#
# KEEP CONFIG (remove binaries and libs, keep config):
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --keep-config
#
# KEEP PATH (don't remove PATH from shell profiles):
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --keep-path
#
# SPECIFIC PREFIX:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --prefix /usr/local
#
# ============================================================
# END OF SEPARATE LINKS SECTION
# ============================================================

# Colors for pretty output
if [ -t 1 ]; then
    RED=$(printf '\033[0;31m')
    GREEN=$(printf '\033[0;32m')
    YELLOW=$(printf '\033[1;33m')
    BLUE=$(printf '\033[0;34m')
    CYAN=$(printf '\033[0;36m')
    BOLD=$(printf '\033[1m')
    NC=$(printf '\033[0m')
else
    RED='' GREEN='' YELLOW='' BLUE='' CYAN='' BOLD='' NC=''
fi

log_info() { printf "%s[INFO]%s %s\n" "$BLUE" "$NC" "$1"; }
log_success() { printf "%s[OK]%s %s\n" "$GREEN" "$NC" "$1"; }
log_warn() { printf "%s[WARN]%s %s\n" "$YELLOW" "$NC" "$1"; }
log_error() { printf "%s[ERROR]%s %s\n" "$RED" "$NC" "$1"; }

# Default installation paths (same as installer)
if [ "$(id -u)" = "0" ]; then
    DEFAULT_PREFIX="/usr/local"
    IS_ROOT=1
    SUDO=""
else
    DEFAULT_PREFIX="$HOME/.local"
    IS_ROOT=0
    SUDO="sudo"
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
Mire Toolchain Uninstaller v4.4.42

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
  curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash
  
  # Force uninstall everything
  curl -fsSL .../uninstall.sh | bash -s -- --force
  
  # Dry run to see what would be removed
  curl -fsSL .../uninstall.sh | bash -s -- --dry-run
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

printf "%s%s%s\n" "${BOLD}${BLUE}================================================${NC}" "" ""
printf "%s%s%s\n" "${BOLD}${BLUE}  Mire Toolchain Uninstaller v4.4.42${NC}" "" ""
printf "%s%s%s\n" "${BOLD}${BLUE}================================================${NC}" "" ""
printf "\n"
log_info "Prefix: ${BOLD}$PREFIX${NC}"
log_info "User: $(whoami)"
printf "\n"

# Collect items to remove (one per line; POSIX sh has no arrays)
TO_REMOVE_FILE=$(mktemp "${TMPDIR:-/tmp}/mire-uninstall.XXXXXX")
trap 'rm -f "$TO_REMOVE_FILE"' EXIT INT TERM

add_item() {
    printf '%s\n' "$1" >> "$TO_REMOVE_FILE"
}

# Binaries
[ -f "$BIN_DIR/mire" ] && add_item "$BIN_DIR/mire"
[ -f "$BIN_DIR/owl" ] && add_item "$BIN_DIR/owl"

# Library directory
if [ "$REMOVE_LIBS" = "1" ] && [ -d "$PREFIX/lib/mire" ]; then
    add_item "$PREFIX/lib/mire"
fi

# Owl home directory
if [ -d "$OWL_HOME" ]; then
    if [ "$REMOVE_LIBS" = "1" ] && [ "$REMOVE_CONFIG" = "1" ]; then
        add_item "$OWL_HOME"
    else
        if [ "$REMOVE_LIBS" = "1" ]; then
            [ -d "$OWL_HOME/libs" ] && add_item "$OWL_HOME/libs"
            [ -d "$OWL_HOME/cache" ] && add_item "$OWL_HOME/cache"
            [ -d "$OWL_HOME/tmp" ] && add_item "$OWL_HOME/tmp"
            [ -d "$OWL_HOME/version" ] && add_item "$OWL_HOME/version"
            [ -d "$OWL_HOME/modules" ] && add_item "$OWL_HOME/modules"
        fi
        if [ "$REMOVE_CONFIG" = "1" ]; then
            [ -f "$OWL_HOME/config.toml" ] && add_item "$OWL_HOME/config.toml"
            [ -d "$OWL_HOME/reg" ] && add_item "$OWL_HOME/reg"
        fi
    fi
fi

# Mire home directory
if [ "$REMOVE_LIBS" = "1" ] && [ -d "$MIRE_HOME" ]; then
    add_item "$MIRE_HOME"
fi

# Shell profile PATH entries
if [ "$REMOVE_PATH" = "1" ]; then
    for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
        if [ -f "$rc" ] && grep -qF "$BIN_DIR" "$rc"; then
            add_item "$rc:$BIN_DIR"
        fi
    done
fi

if [ ! -s "$TO_REMOVE_FILE" ]; then
    log_info "Nothing to remove. Mire toolchain not found at $PREFIX"
    exit 0
fi

printf "%s%s%s\n" "${CYAN}The following will be removed:${NC}" "" ""
while IFS= read -r item; do
    case "$item" in
        *:*)
            file="${item%:*}"
            pattern="${item#*:}"
            printf "  %s%s%s %s\n" "${YELLOW}-${NC}" " " "PATH entry '${BOLD}$pattern${NC}' from $file" ""
            ;;
        *)
            printf "  %s%s%s %s\n" "${YELLOW}-${NC}" " " "$item" ""
            ;;
    esac
done < "$TO_REMOVE_FILE"
printf "\n"

if [ "$DRY_RUN" = "1" ]; then
    log_info "Dry run complete. No changes made."
    exit 0
fi

if [ "$FORCE" != "1" ]; then
    printf "Continue? [y/N] "
    read -r confirm
    case "$confirm" in
        [yY][eE][sS]|[yY]) ;;
        *) printf "Aborted.\n"; exit 1 ;;
    esac
fi

printf "\n"
log_info "Removing..."

# Remove binaries and directories
while IFS= read -r item; do
    case "$item" in
        *:*)
            file="${item%:*}"
            pattern="${item#*:}"
            # Remove PATH export line from shell profile
            grep -vF "$pattern" "$file" > "$file.tmp" 2>/dev/null || true
            mv "$file.tmp" "$file"
            log_success "Removed PATH entry from $file"
            ;;
        *)
            if [ -f "$item" ]; then
                rm -f "$item"
                log_success "Removed $item"
            elif [ -d "$item" ]; then
                rm -rf "$item"
                log_success "Removed directory $item"
            fi
            ;;
    esac
done < "$TO_REMOVE_FILE"

# Clean up empty parent directories
if [ -d "$PREFIX/lib" ] && [ -z "$(ls -A "$PREFIX/lib" 2>/dev/null)" ]; then
    rmdir "$PREFIX/lib" 2>/dev/null && log_success "Removed empty $PREFIX/lib"
fi

if [ -d "$BIN_DIR" ] && [ -z "$(ls -A "$BIN_DIR" 2>/dev/null)" ]; then
    rmdir "$BIN_DIR" 2>/dev/null && log_success "Removed empty $BIN_DIR"
fi

if [ "$IS_ROOT" = "1" ] && [ -d "$PREFIX" ] && [ -z "$(ls -A "$PREFIX" 2>/dev/null)" ]; then
    rmdir "$PREFIX" 2>/dev/null && log_success "Removed empty $PREFIX"
fi

printf "\n"
printf "%s%s%s\n" "${BOLD}${GREEN}================================================${NC}" "" ""
printf "%s%s%s\n" "${BOLD}${GREEN}  Uninstallation complete!${NC}" "" ""
printf "%s%s%s\n" "${BOLD}${GREEN}================================================${NC}" "" ""
printf "\n"
log_info "You may need to restart your shell for PATH changes to take effect."
printf "\n"
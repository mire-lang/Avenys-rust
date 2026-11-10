#!/bin/bash
set -e

# Mire Toolchain Uninstaller v4.4.24
# Removes all components installed by the Mire installer
#
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
#   curl -fsSL .../uninstall.sh | bash -s -- --prefix /usr/local
#
# ============================================================
# END OF SEPARATE LINKS SECTION
# ============================================================

# Colors for pretty output
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    NC='\033[0m'
else
    RED='' GREEN='' YELLOW='' BLUE='' CYAN='' BOLD='' NC=''
fi

# Progress indicator - robust, no garbage
spinner_pid=""
spinner_active=0

# Clean up spinner on exit
cleanup_spinner() {
    if [ "$spinner_active" = "1" ]; then
        kill "$spinner_pid" 2>/dev/null
        wait "$spinner_pid" 2>/dev/null
        # Clear the line
        printf "\r\033[K"
        spinner_active=0
    fi
}
trap cleanup_spinner EXIT INT TERM

start_spinner() {
    local msg="$1"
    # Only use spinner if stdout is a terminal
    if [ ! -t 1 ]; then
        echo -e "${CYAN}[...]${NC} $msg"
        return
    fi
    
    spinner_active=1
    printf "${CYAN}[${NC} ${msg} ${CYAN}]${NC} "
    
    # Run spinner in background with proper cleanup
    (
        local chars='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'
        local i=0
        while true; do
            printf "\r${CYAN}[${NC} %c ${msg} ${CYAN}]${NC} " "${chars:i:1}"
            i=$(( (i + 1) % ${#chars} ))
            sleep 0.1
        done
    ) &
    spinner_pid=$!
}

stop_spinner() {
    local success="$1"
    local msg="$2"
    
    if [ "$spinner_active" = "1" ]; then
        kill "$spinner_pid" 2>/dev/null
        wait "$spinner_pid" 2>/dev/null
        spinner_active=0
    fi
    
    # Clear line and print result
    if [ -t 1 ]; then
        printf "\r\033[K"
    fi
    
    if [ "$success" = "0" ]; then
        echo -e "${GREEN}[✓]${NC} $msg"
    else
        echo -e "${RED}[✗]${NC} $msg"
    fi
}

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

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
Mire Toolchain Uninstaller v4.4.21

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

echo -e "${BOLD}${BLUE}================================================${NC}"
echo -e "${BOLD}${BLUE}  Mire Toolchain Uninstaller v4.4.21${NC}"
echo -e "${BOLD}${BLUE}================================================${NC}"
echo ""
log_info "Prefix: ${BOLD}$PREFIX${NC}"
log_info "User: $(whoami)"
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
        if [ "$REMOVE_LIBS" = "1" ]; then
            [ -d "$OWL_HOME/libs" ] && TO_REMOVE+=("$OWL_HOME/libs")
            [ -d "$OWL_HOME/cache" ] && TO_REMOVE+=("$OWL_HOME/cache")
            [ -d "$OWL_HOME/tmp" ] && TO_REMOVE+=("$OWL_HOME/tmp")
            [ -d "$OWL_HOME/version" ] && TO_REMOVE+=("$OWL_HOME/version")
            [ -d "$OWL_HOME/modules" ] && TO_REMOVE+=("$OWL_HOME/modules")
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
    log_info "Nothing to remove. Mire toolchain not found at $PREFIX"
    exit 0
fi

echo -e "${CYAN}The following will be removed:${NC}"
for item in "${TO_REMOVE[@]}"; do
    if [[ "$item" == *":"* ]]; then
        file="${item%:*}"
        pattern="${item#*:}"
        echo -e "  ${YELLOW}-${NC} PATH entry '${BOLD}$pattern${NC}' from $file"
    else
        echo -e "  ${YELLOW}-${NC} $item"
    fi
done
echo ""

if [ "$DRY_RUN" = "1" ]; then
    log_info "Dry run complete. No changes made."
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
log_info "Removing..."

# Remove binaries and directories
for item in "${TO_REMOVE[@]}"; do
    if [[ "$item" == *":"* ]]; then
        file="${item%:*}"
        pattern="${item#*:}"
        grep -vF "$pattern" "$file" > "$file.tmp" || true
        mv "$file.tmp" "$file"
        log_success "Removed PATH entry from $file"
    elif [ -f "$item" ]; then
        rm -f "$item"
        log_success "Removed $item"
    elif [ -d "$item" ]; then
        rm -rf "$item"
        log_success "Removed directory $item"
    fi
done

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

echo ""
echo -e "${BOLD}${GREEN}================================================${NC}"
echo -e "${BOLD}${GREEN}  Uninstallation complete!${NC}"
echo -e "${BOLD}${GREEN}================================================${NC}"
echo ""
log_info "You may need to restart your shell for PATH changes to take effect."
echo ""
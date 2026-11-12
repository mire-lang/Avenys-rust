#!/bin/bash
set -euo pipefail

# Mire Toolchain Installer v4.4.42
# ============================================================
# SEPARATE INSTALLATION LINKS (copy/paste each as needed)
# ============================================================
#
# QUICK INSTALLS (latest working versions):
#   Quick install (mire + owl + kioto):
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --mireup
#
#   Install mire compiler only:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --compiler
#
#   Install owl package manager only:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --owl
#
#   Install kioto library only:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --kioto
#
#   Install SDL library only:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --sdl
#
#   Install SQLite library only:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --sqlite
#
#   Install WASM library only:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --wasm
#
#   Full interactive install:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --interactive
#
#   Build from source (all components):
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --from-source
#
# SPECIFIC VERSION INSTALLS (replace VERSION with desired tag):
#   Mire compiler v4.4.42:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | MIRE_VERSION=v4.4.42 bash -s -- --compiler
#
#   Owl v1.6.2:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | OWL_VERSION=v1.6.2 bash -s -- --owl
#
#   Both specific versions:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | MIRE_VERSION=v4.4.42 OWL_VERSION=v1.6.2 bash -s -- --mireup
#
# DIRECT RELEASE DOWNLOAD LINKS (manual installation):
#   Mire Compiler:
#     x86_64:   https://github.com/mire-lang/Avenys-rust/releases/download/v4.4.42/mire-compiler-x86_64-unknown-linux-gnu.tar.gz
#     aarch64:  https://github.com/mire-lang/Avenys-rust/releases/download/v4.4.42/mire-compiler-aarch64-unknown-linux-gnu.tar.gz
#     riscv64:  https://github.com/mire-lang/Avenys-rust/releases/download/v4.4.42/mire-compiler-riscv64-unknown-linux-gnu.tar.gz
#     All:      https://github.com/mire-lang/Avenys-rust/releases/tag/v4.4.42
#
#   Owl Package Manager:
#     x86_64:   https://github.com/mire-lang/owl/releases/download/v1.6.2/owl-linux-x86_64.tar.gz
#     aarch64:  https://github.com/mire-lang/owl/releases/download/v1.6.2/owl-linux-aarch64.tar.gz
#     All:      https://github.com/mire-lang/owl/releases/tag/v1.6.2
#
#   Previous versions: replace v4.4.42/v1.6.2 with desired tag
#     https://github.com/mire-lang/Avenys-rust/releases
#     https://github.com/mire-lang/owl/releases
#
# UNINSTALL:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --force
#
# ============================================================
# END OF SEPARATE LINKS SECTION
# ============================================================

# Default versions (can be overridden via env vars)
# NOTE: Use latest KNOWN WORKING releases
DEFAULT_MIRE_VERSION="v4.4.42"
DEFAULT_OWL_VERSION="v1.6.2"
DEFAULT_KIOTO_VERSION="master"

MIRE_VERSION="${MIRE_VERSION:-$DEFAULT_MIRE_VERSION}"
OWL_VERSION="${OWL_VERSION:-$DEFAULT_OWL_VERSION}"
KIOTO_VERSION="${KIOTO_VERSION:-$DEFAULT_KIOTO_VERSION}"

SCRIPT_URL="${MIRE_INSTALL_SCRIPT_URL:-https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh}"

# Colors for pretty output
if [ -t 1 ]; then
    RED=$(printf '\033[0;31m')
    GREEN=$(printf '\033[0;32m')
    YELLOW=$(printf '\033[1;33m')
    BLUE=$(printf '\033[0;34m')
    CYAN=$(printf '\033[0;36m')
    BOLD=$(printf '\033[1m')
    NC=$(printf '\033[0m') # No Color
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
        printf "\r\033[K"
        spinner_active=0
    fi
}
trap cleanup_spinner EXIT INT TERM

start_spinner() {
    local msg="$1"
    if [ ! -t 1 ]; then
        printf "  ... %s\n" "$msg"
        return
    fi
    spinner_active=1
    (
        local chars='|/-\'
        local i=0
        while true; do
            printf "\r\033[K%s[%s]%s %s" "$CYAN" "${chars:i:1}" "$NC" "$msg"
            i=$(( (i + 1) % 4 ))
            sleep 0.1
        done
    ) &
    spinner_pid=$!
}

stop_spinner() {
    local success="${1:-0}"
    local msg="${2:-}"
    if [ "$spinner_active" = "1" ]; then
        kill "$spinner_pid" 2>/dev/null || true
        wait "$spinner_pid" 2>/dev/null || true
        spinner_active=0
        if [ -t 1 ]; then
            printf "\r\033[K"
        fi
    fi
    if [ "$success" = "0" ]; then
        printf "%s[OK]%s %s\n" "$GREEN" "$NC" "$msg"
    else
        printf "%s[FAILED]%s %s\n" "$RED" "$NC" "$msg"
    fi
}

log_info() { printf "%s[INFO]%s %s\n" "$BLUE" "$NC" "$1"; }
log_success() { printf "%s[OK]%s %s\n" "$GREEN" "$NC" "$1"; }
log_warn() { printf "%s[WARN]%s %s\n" "$YELLOW" "$NC" "$1"; }
log_error() { printf "%s[ERROR]%s %s\n" "$RED" "$NC" "$1"; }

# Detect if running as root or user
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

# Make freshly installed tools visible to the rest of the script
export PATH="$BIN_DIR:$PATH"

# Version variables (can be overridden)
MIRE_VERSION="${MIRE_VERSION:-$DEFAULT_MIRE_VERSION}"
OWL_VERSION="${OWL_VERSION:-$DEFAULT_OWL_VERSION}"
KIOTO_VERSION="${KIOTO_VERSION:-$DEFAULT_KIOTO_VERSION}"

usage() {
    cat << 'USAGE'
Mire Toolchain Installer v4.4.42

Usage:
  install.sh [options]

Options:
  --mireup              Quick default install (mire + owl + kioto)
  --interactive         Interactive menu to select components
  --from-source         Build everything from source
  --compiler            Install Mire compiler
  --owl                 Install Owl package manager
  --kioto               Install Kioto library
  --sdl                 Install SDL library
  --sqlite              Install SQLite library
  --wasm                Install WASM library
  --prefix <path>       Install prefix (default: /usr/local or ~/.local)
  --yes, -y             Non-interactive (assume yes)
  --help, -h            Show this help

Environment Variables:
  MIRE_VERSION=vX.Y.Z   Specific mire version to install (default: v4.4.42)
  OWL_VERSION=vX.Y.Z    Specific owl version to install (default: v1.6.2)
  KIOTO_VERSION=branch  Specific kioto branch/tag (default: master)

Examples:
  # Quick install (mire + owl + kioto) - latest versions
  curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --mireup

  # Install specific versions
  MIRE_VERSION=v4.4.42 OWL_VERSION=v1.6.2 curl -fsSL .../install.sh | bash -s -- --mireup

  # Install only mire compiler v4.4.17
  MIRE_VERSION=v4.4.17 curl -fsSL .../install.sh | bash -s -- --compiler

  # Interactive selection
  curl -fsSL .../install.sh | bash -s -- --interactive

  # Build from source
  curl -fsSL .../install.sh | bash -s -- --from-source
USAGE
}

MIREUP=0
INTERACTIVE=0
FROM_SOURCE=0
INSTALL_COMPILER=0
INSTALL_OWL=0
INSTALL_KIOTO=0
INSTALL_SDL=0
INSTALL_SQLITE=0
INSTALL_WASM=0
YES=0

while [ $# -gt 0 ]; do
    case "$1" in
        --mireup) MIREUP=1; INSTALL_COMPILER=1; INSTALL_OWL=1; INSTALL_KIOTO=1; shift ;;
        --interactive) INTERACTIVE=1; shift ;;
        --from-source) FROM_SOURCE=1; shift ;;
        --compiler) INSTALL_COMPILER=1; shift ;;
        --owl) INSTALL_OWL=1; shift ;;
        --kioto) INSTALL_KIOTO=1; shift ;;
        --sdl) INSTALL_SDL=1; shift ;;
        --sqlite) INSTALL_SQLITE=1; shift ;;
        --wasm) INSTALL_WASM=1; shift ;;
        --prefix) PREFIX="$2"; BIN_DIR="${PREFIX}/bin"; shift 2 ;;
        --yes|-y) YES=1; shift ;;
        --help|-h) usage; exit 0 ;;
        *) shift ;;
    esac
done

# Print header
printf "%s%s%s\n" "${BOLD}${BLUE}================================================${NC}" "" ""
printf "%s%s%s\n" "${BOLD}${BLUE}  Mire Toolchain Installer v4.4.42${NC}" "" ""
printf "%s%s%s\n" "${BOLD}${BLUE}================================================${NC}" "" ""
printf "\n"
log_info "Prefix: ${BOLD}$PREFIX${NC}"
log_info "User: $(whoami)"
printf "\n"

# Ensure directories
mkdir -p "$BIN_DIR" "$OWL_HOME" "$OWL_HOME/libs" "$OWL_HOME/modules" "$MIRE_HOME"
mkdir -p "$PREFIX/lib/mire"

# Detect if updating or fresh install
UPDATING=0
if [ -f "$BIN_DIR/mire" ] || [ -f "$BIN_DIR/owl" ]; then
    UPDATING=1
    log_info "Existing installation detected - will update components"
fi

if [ "$INTERACTIVE" = "1" ]; then
    printf "%s%s%s\n" "${CYAN}Select components to install (space/comma separated):${NC}" "" ""
    printf "  1) Avenys (mire compiler)\n"
    printf "  2) Owl (package manager)\n"
    printf "  3) Kioto (library)\n"
    printf "  4) SDL (library)\n"
    printf "  5) SQLite (library)\n"
    printf "  6) WASM (library)\n"
    printf "\n"
    printf "Enter numbers (e.g. 1 2 3 or 1,2,3): "
    read -r selection
    selection=$(echo "$selection" | tr ',' ' ')
    INSTALL_COMPILER=0
    INSTALL_OWL=0
    INSTALL_KIOTO=0
    INSTALL_SDL=0
    INSTALL_SQLITE=0
    INSTALL_WASM=0
    for s in $selection; do
        case "$s" in
            1) INSTALL_COMPILER=1 ;;
            2) INSTALL_OWL=1 ;;
            3) INSTALL_KIOTO=1 ;;
            4) INSTALL_SDL=1 ;;
            5) INSTALL_SQLITE=1 ;;
            6) INSTALL_WASM=1 ;;
        esac
    done
    printf "\n"
fi

# Function to fix binary RPATH for bundled libraries
fix_binary_rpath() {
    local binary="$1"
    local lib_dir="$2"
    local rel_path='$ORIGIN/../lib/mire'

    if command -v patchelf >/dev/null 2>&1; then
        patchelf --set-rpath "$rel_path" "$binary" 2>/dev/null && return 0 || true
    fi

    # Fallback: create a wrapper script that sets LD_LIBRARY_PATH
    local real_binary="${binary}.bin"
    mv "$binary" "$real_binary"
    cat > "$binary" << WRAPEOF
#!/bin/sh
# Wrapper for $(basename "$binary") - sets library path for bundled libsodium
export LD_LIBRARY_PATH="${lib_dir}\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
exec "${real_binary}" "\$@"
WRAPEOF
    chmod +x "$binary"
    return 0
}

# Function to install system dependencies
install_deps() {
    local deps_type="$1"
    start_spinner "Installing system dependencies ($deps_type)"
    
    if command -v apt-get >/dev/null 2>&1; then
        $SUDO apt-get update -qq
        case "$deps_type" in
            build)
                $SUDO apt-get install -y -qq curl git build-essential clang llvm lld pkg-config \
                    libssl-dev libsodium-dev zlib1g-dev libzstd-dev libarchive-dev tcc patchelf
                ;;
            runtime)
                $SUDO apt-get install -y -qq \
                    libsodium23 libzstd-dev libarchive-dev zlib1g-dev tcc libtcc-dev libsqlite3-dev
                ;;
        esac
    elif command -v pacman >/dev/null 2>&1; then
        case "$deps_type" in
            build)
                $SUDO pacman -S --needed --noconfirm curl git base-devel clang llvm lld openssl \
                    libsodium zlib zstd libarchive tcc patchelf
                ;;
            runtime)
                $SUDO pacman -S --needed --noconfirm libsodium zlib zstd libarchive tcc sqlite
                ;;
        esac
    elif command -v dnf >/dev/null 2>&1; then
        case "$deps_type" in
            build)
                # Try to install available packages, ignore missing ones
                $SUDO dnf install -y curl git gcc gcc-c++ clang llvm llvm-devel lld cargo \
                    pkgconf-pkg-config openssl-devel zlib-devel zstd-devel \
                    libarchive-devel patchelf 2>/dev/null || true
                # Try to install libsodium-devel if available
                $SUDO dnf install -y libsodium-devel 2>/dev/null || true
                # Install tcc from source if not available
                if ! command -v tcc >/dev/null 2>&1; then
                    start_spinner "Installing tcc from source"
                    cd /tmp && rm -rf tinycc && git clone --depth 1 https://github.com/TinyCC/tinycc.git 2>/dev/null && cd tinycc && ./configure --prefix=/usr/local 2>/dev/null && make -j$(nproc) 2>/dev/null && sudo make install 2>/dev/null
                    stop_spinner 0 "tcc installed from source"
                fi
                # Install libsodium from source if not available
                if ! pkg-config --exists libsodium 2>/dev/null; then
                    start_spinner "Installing libsodium from source"
                    cd /tmp && rm -rf libsodium && git clone --depth 1 --branch stable https://github.com/jedisct1/libsodium.git 2>/dev/null && cd libsodium && ./configure --prefix=/usr/local 2>/dev/null && make -j$(nproc) 2>/dev/null && sudo make install 2>/dev/null
                    stop_spinner 0 "libsodium installed from source"
                fi
                ;;
            runtime)
                $SUDO dnf install -y zlib zstd libarchive sqlite 2>/dev/null || true
                # Install tcc from source if not available
                if ! command -v tcc >/dev/null 2>&1; then
                    start_spinner "Installing tcc from source"
                    cd /tmp && rm -rf tinycc && git clone --depth 1 https://github.com/TinyCC/tinycc.git 2>/dev/null && cd tinycc && ./configure --prefix=/usr/local 2>/dev/null && make -j$(nproc) 2>/dev/null && sudo make install 2>/dev/null
                    stop_spinner 0 "tcc installed from source"
                fi
                # Install libsodium from source if not available
                if ! pkg-config --exists libsodium 2>/dev/null; then
                    start_spinner "Installing libsodium from source"
                    cd /tmp && rm -rf libsodium && git clone --depth 1 --branch stable https://github.com/jedisct1/libsodium.git 2>/dev/null && cd libsodium && ./configure --prefix=/usr/local 2>/dev/null && make -j$(nproc) 2>/dev/null && sudo make install 2>/dev/null
                    stop_spinner 0 "libsodium installed from source"
                fi
                ;;
        esac
    elif command -v zypper >/dev/null 2>&1; then
        case "$deps_type" in
            build)
                $SUDO zypper install -y curl git gcc gcc-c++ clang llvm llvm-devel lld rust \
                    cargo libopenssl-devel libsodium-devel zlib-devel libzstd-devel \
                    libarchive-devel patchelf
                ;;
            runtime)
                $SUDO zypper install -y libsodium zlib zstd libarchive tcc sqlite
                ;;
        esac
    fi
    stop_spinner 0 "System dependencies installed"
}

# Function to install Rust
install_rust() {
    if ! command -v rustc >/dev/null 2>&1; then
        start_spinner "Installing Rust toolchain"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
        export PATH="$HOME/.cargo/bin:$PATH"
        stop_spinner 0 "Rust toolchain installed"
    fi
}

# Function to configure MOR registry
configure_mor_registry() {
    start_spinner "Configuring MOR registry"

    local mor_dir="$OWL_HOME/reg/mor"
    local index_url="https://raw.githubusercontent.com/mire-lang/libs/main/reg/index.toml"
    local meta_url="https://raw.githubusercontent.com/mire-lang/libs/main/reg/registry.json"

    mkdir -p "$mor_dir" "$OWL_HOME/cfg" "$OWL_HOME/libs"
    printf '1\n' > "$mor_dir/protocol"
    printf "%s\n" "https://github.com/mire-lang/libs/raw/main/reg" > "$mor_dir/source_url"

    # Prefer the published index; fall back to a minimal offline copy.
    if ! curl -fsSL "$index_url" -o "$mor_dir/index.toml"; then
        cat > "$mor_dir/index.toml" << 'EOF'
[[packages]]
name = "mire"
version = "2.1.0-beta"
method = "clone"
repo = "mire-lang/mire-lib"
ref = "main"

[[packages]]
name = "kioto"
version = "3.1.0"
method = "clone"
repo = "mire-lang/Kioto"
ref = "main"

[[packages]]
name = "sdl"
version = "1.0.1"
method = "clone"
repo = "mire-lang/SDL"
ref = "master"

[[packages]]
name = "sqlite"
version = "0.1.0"
method = "clone"
repo = "mire-lang/sqlite"
ref = "master"

[[packages]]
name = "wasm"
version = "0.2.0"
method = "clone"
repo = "mire-lang/wasm"
ref = "main"

[[packages]]
name = "ilogical"
version = "0.1.0"
method = "clone"
repo = "mire-lang/ilogical"
ref = "v0.1.0"
EOF
    fi
    curl -fsSL "$meta_url" -o "$mor_dir/registry.json" 2>/dev/null || true

    # Global owl configuration (owl regenerates it if missing).
    if [ ! -f "$OWL_HOME/cfg/global.toml" ]; then
        cat > "$OWL_HOME/cfg/global.toml" << 'EOF'
[registry]
default = "MOR"
required = true

[lock]
version = 3
update = "on-resolve"
strict = false

[build]
default-profile = "debug"
default-target = "SystemAuto"

[projects]
deps-path = "~/.owl/libs"
src = "./src/"
target = "./bin/"
cache = "./bin/.cache"
EOF
    fi

    stop_spinner 0 "MOR registry configured"
}

# Function to install mire compiler
install_mire() {
    local version="$1"
    start_spinner "Installing mire compiler $version"

    local arch triple tmpdir url
    arch=$(uname -m)
    case "$arch" in
        x86_64) triple="x86_64-unknown-linux-gnu" ;;
        aarch64|arm64) triple="aarch64-unknown-linux-gnu" ;;
        riscv64) triple="riscv64-unknown-linux-gnu" ;;
        *) triple="x86_64-unknown-linux-gnu" ;;
    esac

    tmpdir=$(mktemp -d)
    url="https://github.com/mire-lang/Avenys-rust/releases/download/${version}/mire-compiler-${triple}.tar.gz"

    if ! curl -fsSL "$url" -o "$tmpdir/mire.tar.gz"; then
        rm -rf "$tmpdir"
        stop_spinner 1 "Failed to download mire compiler $version"
        return 1
    fi

    if ! tar xzf "$tmpdir/mire.tar.gz" -C "$tmpdir"; then
        rm -rf "$tmpdir"
        stop_spinner 1 "Failed to extract mire compiler $version"
        return 1
    fi

    if [ ! -f "$tmpdir/compiler/mire" ]; then
        rm -rf "$tmpdir"
        stop_spinner 1 "mire binary missing from the $version archive"
        return 1
    fi

    mkdir -p "$BIN_DIR" "$PREFIX/lib/mire"
    cp "$tmpdir/compiler/mire" "$BIN_DIR/mire"
    chmod +x "$BIN_DIR/mire"
    if [ -d "$tmpdir/compiler/runtime" ]; then
        cp -r "$tmpdir/compiler/runtime" "$PREFIX/lib/mire/"
    fi
    if [ -d "$tmpdir/compiler/pal" ]; then
        cp -r "$tmpdir/compiler/pal" "$PREFIX/lib/mire/"
    fi
    fix_binary_rpath "$BIN_DIR/mire" "$PREFIX/lib/mire"
    rm -rf "$tmpdir"

    stop_spinner 0 "Mire compiler installed ($version)"
}

# Function to install owl package manager
install_owl() {
    local version="$1"
    start_spinner "Installing owl package manager $version"

    local arch triple tmpdir url owl_bin
    arch=$(uname -m)
    case "$arch" in
        x86_64) triple="linux-x86_64" ;;
        aarch64|arm64) triple="linux-aarch64" ;;
        *) triple="linux-x86_64" ;;
    esac

    tmpdir=$(mktemp -d)
    url="https://github.com/mire-lang/owl/releases/download/${version}/owl-${triple}.tar.gz"

    if ! curl -fsSL "$url" -o "$tmpdir/owl.tar.gz"; then
        rm -rf "$tmpdir"
        stop_spinner 1 "Failed to download owl $version"
        return 1
    fi

    if ! tar xzf "$tmpdir/owl.tar.gz" -C "$tmpdir"; then
        rm -rf "$tmpdir"
        stop_spinner 1 "Failed to extract owl $version"
        return 1
    fi

    owl_bin="$tmpdir/owl/owl"
    if [ ! -f "$owl_bin" ]; then
        owl_bin=$(find "$tmpdir" -name owl -type f 2>/dev/null | head -1)
    fi
    if [ -z "$owl_bin" ] || [ ! -f "$owl_bin" ]; then
        rm -rf "$tmpdir"
        stop_spinner 1 "owl binary missing from the $version archive"
        return 1
    fi

    mkdir -p "$BIN_DIR" "$PREFIX/lib/mire" "$OWL_HOME/libs"
    cp "$owl_bin" "$BIN_DIR/owl"
    chmod +x "$BIN_DIR/owl"

    if [ -d "$tmpdir/owl/lib" ]; then
        cp -r "$tmpdir/owl/lib/." "$PREFIX/lib/mire/" 2>/dev/null || true
    fi
    if [ -d "$tmpdir/owl/kioto" ] && [ ! -e "$OWL_HOME/libs/kioto" ]; then
        cp -r "$tmpdir/owl/kioto" "$OWL_HOME/libs/kioto" 2>/dev/null || true
    fi

    fix_binary_rpath "$BIN_DIR/owl" "$PREFIX/lib/mire"
    rm -rf "$tmpdir"

    stop_spinner 0 "Owl package manager installed ($version)"
}

# Function to install libraries via owl
install_via_owl() {
    local pkg="$1"
    local log
    log=$(mktemp)
    start_spinner "Installing $pkg via owl"
    if owl install "$pkg" >"$log" 2>&1; then
        rm -f "$log"
        stop_spinner 0 "$pkg installed"
    else
        stop_spinner 1 "$pkg failed"
        printf "    %s\n" "--- owl install $pkg output (last 15 lines) ---"
        tail -n 15 "$log" | sed 's/^/    /'
        rm -f "$log"
    fi
}

install_libraries_via_owl() {
    if ! command -v owl >/dev/null 2>&1; then
        log_warn "owl not found, skipping library installation via owl"
        return
    fi

    export PATH="$BIN_DIR:$PATH"

    # Ensure MOR registry is configured
    configure_mor_registry

    # Refresh the index from mire-lang/libs when online.
    local log
    log=$(mktemp)
    start_spinner "Syncing MOR registry"
    if owl reg sync >"$log" 2>&1; then
        rm -f "$log"
        stop_spinner 0 "Registry synced"
    else
        rm -f "$log"
        stop_spinner 0 "Registry kept (offline or sync failed)"
    fi

    if [ "$INSTALL_KIOTO" = "1" ] || [ "$MIREUP" = "1" ]; then
        install_via_owl kioto
    fi
    if [ "$INSTALL_SDL" = "1" ]; then
        install_via_owl sdl
    fi
    if [ "$INSTALL_SQLITE" = "1" ]; then
        install_via_owl sqlite
    fi
    if [ "$INSTALL_WASM" = "1" ]; then
        install_via_owl wasm
    fi
    if [ "$MIREUP" = "1" ]; then
        install_via_owl mire
    fi
}

# Function to install libraries directly (fallback)
install_libraries_direct() {
    printf "Installing libraries to %s...\n" "$OWL_HOME/libs"
    mkdir -p "$OWL_HOME/libs"
    
    for lib in kioto mire sdl sqlite wasm; do
        if [ ! -d "$OWL_HOME/libs/$lib" ] && [ ! -d "$OWL_HOME/libs/$lib@*" ]; then
            case "$lib" in
                kioto)
                    start_spinner "Installing kioto from source"
                    curl -fsSL "https://github.com/mire-lang/Kioto/archive/refs/heads/${KIOTO_VERSION}.tar.gz" -o /tmp/kioto.tar.gz 2>/dev/null && \
                    tar xzf /tmp/kioto.tar.gz -C /tmp && \
                    for d in /tmp/Kioto-*/; do [ -f "$d/owl.toml" ] && cp -r "$d" "$OWL_HOME/libs/kioto" && break; done && \
                    rm -rf /tmp/kioto.tar.gz /tmp/Kioto-* 2>/dev/null || true
                    stop_spinner 0 "kioto installed"
                    ;;
                mire)
                    start_spinner "Installing mire stdlib from source"
                    curl -fsSL "https://github.com/mire-lang/mire-lib/archive/refs/heads/main.tar.gz" -o /tmp/mire.tar.gz 2>/dev/null && \
                    tar xzf /tmp/mire.tar.gz -C /tmp && \
                    for d in /tmp/mire-lib-*/; do cp -r "$d" "$OWL_HOME/libs/mire" 2>/dev/null && break; done && \
                    rm -rf /tmp/mire.tar.gz /tmp/mire-lib-* 2>/dev/null || true
                    stop_spinner 0 "mire stdlib installed"
                    ;;
            esac
        fi
    done
}

# Function to add to PATH
add_to_path() {
    if [ "$NO_PROFILE" != "1" ]; then
        for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
            if [ -f "$rc" ] && ! grep -q "$BIN_DIR" "$rc"; then
                printf "export PATH=\"%s:\$PATH\"\n" "$BIN_DIR" >> "$rc"
                log_info "Added $BIN_DIR to $rc"
                break
            fi
        done
    fi
}

# ============================================================
# MAIN INSTALLATION FLOW
# ============================================================

if [ "$FROM_SOURCE" = "1" ]; then
    printf "%s%s%s\n" "${BOLD}${YELLOW}=== Building from source ===${NC}" "" ""
    printf "\n"
    install_deps "build"
    install_rust
    
    TMPDIR_SRC=$(mktemp -d)
    cd "$TMPDIR_SRC"
    
    printf "Cloning repositories...\n"
    start_spinner "Cloning Avenys-rust"
    git clone --depth 1 https://github.com/mire-lang/Avenys-rust avenys 2>/dev/null || true
    stop_spinner 0 "Cloned Avenys-rust"
    
    start_spinner "Cloning owl"
    git clone --depth 1 https://github.com/mire-lang/owl owl 2>/dev/null || true
    stop_spinner 0 "Cloned owl"
    
    start_spinner "Cloning Kioto"
    git clone --depth 1 https://github.com/mire-lang/Kioto kioto 2>/dev/null || true
    stop_spinner 0 "Cloned Kioto"
    
    start_spinner "Cloning mire-lib"
    git clone --depth 1 https://github.com/mire-lang/mire-lib mire-lib 2>/dev/null || true
    stop_spinner 0 "Cloned mire-lib"
    
    if [ -d avenys ]; then
        printf "\n"
        log_info "Building mire compiler..."
        cd avenys
        LLVM_CONFIG_PATH=$(which llvm-config-22 2>/dev/null || which llvm-config 2>/dev/null) cargo build --release
        if [ -f target/release/mire ]; then
            cp target/release/mire "$BIN_DIR/mire"
            chmod +x "$BIN_DIR/mire"
            mkdir -p "$PREFIX/lib/mire"
            cp -r src/runtime "$PREFIX/lib/mire/" 2>/dev/null || true
            cp -r src/pal "$PREFIX/lib/mire/" 2>/dev/null || true
            log_success "mire compiler built and installed"
        fi
        cd ..
    fi
    
    if [ -d owl ]; then
        printf "\n"
        log_info "Building owl..."
        export PATH="$BIN_DIR:$PATH"
        if command -v mire >/dev/null 2>&1; then
            cd owl
            mkdir -p "$OWL_HOME/libs/kioto" "$OWL_HOME/libs/mire"
            cp -r ../kioto/* "$OWL_HOME/libs/kioto/" 2>/dev/null || true
            cp -r ../mire-lib/* "$OWL_HOME/libs/mire/" 2>/dev/null || true
            cat > native/mire-config.toml << CFGEOF
[project]
name = "owl"
version = "$OWL_VERSION"
entry = "code/main.mire"

[build]
profile = "release"
opt-level = 3
artifact = "bin"
runtime = "minimal"

[paths]
bin = "bin"
cache = "bin/.cache"

[cfg]
libs = []
sources = []
link-dirs = []
cflags = []
CFGEOF
            mire build code/main.mire --release -O3 --lib-dir "$OWL_HOME/libs" --config native/mire-config.toml 2>/dev/null || true
            if [ -f bin/main ] || [ -f bin/release/main ]; then
                [ -f bin/release/main ] && cp bin/release/main "$BIN_DIR/owl" || cp bin/main "$BIN_DIR/owl"
                chmod +x "$BIN_DIR/owl"
                fix_binary_rpath "$BIN_DIR/owl" "$PREFIX/lib/mire"
                log_success "owl built and installed"
            fi
            cd ..
        fi
    fi
    
    rm -rf "$TMPDIR_SRC"
    
else
    # Download from releases
    install_deps "runtime"
    
    printf "\n"
    if [ "$INSTALL_COMPILER" = "1" ] || [ "$MIREUP" = "1" ]; then
        install_mire "$MIRE_VERSION"
    fi
    
    if [ "$INSTALL_OWL" = "1" ] || [ "$MIREUP" = "1" ]; then
        install_owl "$OWL_VERSION"
    fi
    
    # Install libraries via owl (preferred) or direct
    if command -v owl >/dev/null 2>&1; then
        install_libraries_via_owl
    else
        # Install libraries directly if owl not available
        if [ "$INSTALL_KIOTO" = "1" ] || [ "$MIREUP" = "1" ] || [ "$INSTALL_SDL" = "1" ] || [ "$INSTALL_SQLITE" = "1" ] || [ "$INSTALL_WASM" = "1" ]; then
            install_libraries_direct
        fi
    fi
fi

# Add to PATH
add_to_path

# Final summary
printf "\n"
printf "%s%s%s\n" "${BOLD}${BLUE}================================================${NC}" "" ""
printf "%s%s%s\n" "${BOLD}${BLUE}  Installation Complete!${NC}" "" ""
printf "%s%s%s\n" "${BOLD}${BLUE}================================================${NC}" "" ""

if [ -f "$BIN_DIR/owl" ]; then
    OWL_VER=$("$BIN_DIR/owl" -V 2>/dev/null | head -1 || echo "installed")
    log_success "owl: $OWL_VER"
fi
if [ -f "$BIN_DIR/mire" ]; then
    MIRE_VER=$("$BIN_DIR/mire" --version 2>/dev/null | head -1 || echo "installed")
    log_success "mire: $MIRE_VER"
fi

printf "\n"
printf "%s%s%s\n" "${CYAN}Next steps:${NC}" "" ""
printf "  1. Restart your shell or run: export PATH=\$PATH:%s\n" "$BIN_DIR"
printf "  2. Test: mire --version && owl --version\n"
printf "  3. Install libraries: owl install mire kioto ilogical\n"
printf "\n"
if [ "$UPDATING" = "1" ]; then
    log_info "Updated existing installation"
else
    log_info "Fresh installation complete"
fi
printf "\n"
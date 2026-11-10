#!/bin/bash
set -e

# Mire Toolchain Installer v4.4.29
# ============================================================
# SEPARATE INSTALLATION LINKS (copy/paste each as needed)
# ============================================================
#
# QUICK INSTALLS (latest versions):
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
#   Mire compiler v4.4.29:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | MIRE_VERSION=v4.4.29 bash -s -- --compiler
#
#   Owl v1.6.2:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | OWL_VERSION=v1.6.2 bash -s -- --owl
#
#   Both specific versions:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | MIRE_VERSION=v4.4.29 OWL_VERSION=v1.6.2 bash -s -- --mireup
#
# DIRECT RELEASE DOWNLOAD LINKS (manual installation):
#   Mire Compiler:
#     x86_64:   curl -fsSL https://github.com/mire-lang/Avenys-rust/releases/download/v4.4.29/mire-compiler-x86_64-unknown-linux-gnu.tar.gz
#     aarch64:  curl -fsSL https://github.com/mire-lang/Avenys-rust/releases/download/v4.4.29/mire-compiler-aarch64-unknown-linux-gnu.tar.gz
#     riscv64:  curl -fsSL https://github.com/mire-lang/Avenys-rust/releases/download/v4.4.29/mire-compiler-riscv64-unknown-linux-gnu.tar.gz
#     All:      https://github.com/mire-lang/Avenys-rust/releases/tag/v4.4.29
#
#   Owl Package Manager:
#     x86_64:   curl -fsSL https://github.com/mire-lang/owl/releases/download/v1.6.2/owl-linux-x86_64.tar.gz
#     aarch64:  curl -fsSL https://github.com/mire-lang/owl/releases/download/v1.6.2/owl-linux-aarch64.tar.gz
#     All:      https://github.com/mire-lang/owl/releases/tag/v1.6.2
#
#   Previous versions: replace v4.4.29/v1.6.2 with desired tag
#     https://github.com/mire-lang/Avenys-rust/releases
#     https://github.com/mire-lang/owl/releases
#
# UNINSTALL:
#   curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash
#
# ============================================================
# END OF SEPARATE LINKS SECTION
# ============================================================

# Default versions (can be overridden via env vars)
DEFAULT_MIRE_VERSION="${MIRE_VERSION:-v4.4.29}"
DEFAULT_OWL_VERSION="${OWL_VERSION:-v1.6.2}"
DEFAULT_KIOTO_VERSION="${KIOTO_VERSION:-master}"

SCRIPT_URL="${MIRE_INSTALL_SCRIPT_URL:-https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh}"

# Colors for pretty output
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    NC='\033[0m' # No Color
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

# Version variables (can be overridden)
MIRE_VERSION="${MIRE_VERSION:-$DEFAULT_MIRE_VERSION}"
OWL_VERSION="${OWL_VERSION:-$DEFAULT_OWL_VERSION}"
KIOTO_VERSION="${KIOTO_VERSION:-$DEFAULT_KIOTO_VERSION}"

usage() {
    cat << 'USAGE'
Mire Toolchain Installer v4.4.29

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
  MIRE_VERSION=vX.Y.Z   Specific mire version to install (default: v4.4.21)
  OWL_VERSION=vX.Y.Z    Specific owl version to install (default: v1.6.2)
  KIOTO_VERSION=branch  Specific kioto branch/tag (default: master)

Examples:
  # Quick install (mire + owl + kioto) - latest versions
  curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --mireup

  # Install specific versions
  MIRE_VERSION=v4.4.29 OWL_VERSION=v1.6.2 curl -fsSL .../install.sh | bash -s -- --mireup

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
echo -e "${BOLD}${BLUE}================================================${NC}"
echo -e "${BOLD}${BLUE}  Mire Toolchain Installer v4.4.29${NC}"
echo -e "${BOLD}${BLUE}================================================${NC}"
echo ""
log_info "Prefix: ${BOLD}$PREFIX${NC}"
log_info "User: $(whoami)"
echo ""

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
    echo -e "${CYAN}Select components to install (space/comma separated):${NC}"
    echo "  1) Avenys (mire compiler)"
    echo "  2) Owl (package manager)"
    echo "  3) Kioto (library)"
    echo "  4) SDL (library)"
    echo "  5) SQLite (library)"
    echo "  6) WASM (library)"
    echo ""
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
    echo ""
fi

# Function to fix binary RPATH for bundled libraries
fix_binary_rpath() {
    local binary="$1"
    local lib_dir="$2"
    local rel_path='$ORIGIN/../lib/mire'

    if command -v patchelf >/dev/null 2>&1; then
        patchelf --set-rpath "$rel_path" "$binary" 2>/dev/null && return 0
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
                $SUDO dnf install -y curl git gcc gcc-c++ clang llvm llvm-devel lld cargo \
                    pkgconf-pkg-config openssl-devel libsodium-devel zlib-devel zstd-devel \
                    libarchive-devel patchelf
                ;;
            runtime)
                $SUDO dnf install -y libsodium zlib zstd libarchive tcc sqlite
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
    
    mkdir -p "$OWL_HOME/reg"
    
    # Create registry index pointing to mire-lang/libs
    cat > "$OWL_HOME/reg/index.toml" << 'EOF'
# MOR (Mire Official Registry) - Official mire-lang libraries
# This registry is pre-configured to use github.com/mire-lang/libs

[registry]
name = "MOR"
description = "Official Mire Language Package Registry"
url = "https://github.com/mire-lang/libs"
type = "git"
format = "index"

# Default source for packages
default_source = "github"

# Package sources
[sources.github]
type = "github"
org = "mire-lang"
repo = "libs"
branch = "main"
path = "packages"

# Alternative: direct tarball downloads
[sources.releases]
type = "github-releases"
org = "mire-lang"
repo = "libs"
EOF

    # Also create config.toml for owl
    cat > "$OWL_HOME/config.toml" << EOF
[registries]
mor = { url = "https://github.com/mire-lang/libs", type = "git" }

[defaults]
registry = "mor"
EOF

    stop_spinner 0 "MOR registry configured"
}

# Function to install mire compiler
install_mire() {
    local version="$1"
    start_spinner "Installing mire compiler $version"
    
    ARCH=$(uname -m)
    case "$ARCH" in
        x86_64) TRIPLE="x86_64-unknown-linux-gnu" ;;
        aarch64) TRIPLE="aarch64-unknown-linux-gnu" ;;
        *) TRIPLE="x86_64-unknown-linux-gnu" ;;
    esac
    
    # Download from releases
    URL="https://github.com/mire-lang/Avenys-rust/releases/download/${version}/mire-compiler-${TRIPLE}.tar.gz"
    curl -fsSL "$URL" -o /tmp/mire.tar.gz 2>/dev/null
    
    if [ -f /tmp/mire.tar.gz ]; then
        tar xzf /tmp/mire.tar.gz -C /tmp
        if [ -f /tmp/compiler/mire ]; then
            cp /tmp/compiler/mire "$BIN_DIR/mire"
            chmod +x "$BIN_DIR/mire"
            mkdir -p "$PREFIX/lib/mire"
            [ -d /tmp/compiler/runtime ] && cp -r /tmp/compiler/runtime "$PREFIX/lib/mire/"
            [ -d /tmp/compiler/pal ] && cp -r /tmp/compiler/pal "$PREFIX/lib/mire/"
            log_success "mire compiler installed ($version)"
        fi
        rm -rf /tmp/compiler /tmp/mire.tar.gz
    else
        log_error "Failed to download mire compiler $version"
        return 1
    fi
    stop_spinner 0 "Mire compiler installed"
}

# Function to install owl package manager
install_owl() {
    local version="$1"
    start_spinner "Installing owl package manager $version"
    
    ARCH=$(uname -m)
    case "$ARCH" in
        x86_64) TRIPLE="linux-x86_64" ;;
        aarch64) TRIPLE="linux-aarch64" ;;
        *) TRIPLE="linux-x86_64" ;;
    esac
    
    # Download from releases
    URL="https://github.com/mire-lang/owl/releases/download/${version}/owl-${TRIPLE}.tar.gz"
    curl -fsSL "$URL" -o /tmp/owl.tar.gz 2>/dev/null
    
    if [ -f /tmp/owl.tar.gz ]; then
        tar xzf /tmp/owl.tar.gz -C /tmp
        if [ -f /tmp/owl/owl ]; then
            cp /tmp/owl/owl "$BIN_DIR/owl"
            chmod +x "$BIN_DIR/owl"
            [ -d /tmp/owl/lib ] && cp /tmp/owl/lib/* "$PREFIX/lib/mire/" 2>/dev/null || true
            [ -d /tmp/owl/kioto ] && cp -r /tmp/owl/kioto "$OWL_HOME/libs/" 2>/dev/null || true
            fix_binary_rpath "$BIN_DIR/owl" "$PREFIX/lib/mire"
            log_success "owl package manager installed ($version)"
        fi
        rm -rf /tmp/owl /tmp/owl.tar.gz
    else
        log_error "Failed to download owl $version"
        return 1
    fi
    stop_spinner 0 "Owl package manager installed"
}

# Function to install libraries via owl
install_libraries() {
    if ! command -v owl >/dev/null 2>&1; then
        log_warn "owl not found, skipping library installation"
        return
    fi
    
    export PATH="$BIN_DIR:$PATH"
    
    # Ensure MOR registry is configured
    configure_mor_registry
    
    # Sync registry
    start_spinner "Syncing MOR registry"
    owl reg 2>/dev/null || true
    stop_spinner 0 "Registry synced"
    
    # Install requested libraries
    if [ "$INSTALL_KIOTO" = "1" ] || [ "$MIREUP" = "1" ]; then
        start_spinner "Installing kioto library"
        owl install kioto 2>/dev/null || true
        stop_spinner 0 "kioto installed"
    fi
    if [ "$INSTALL_SDL" = "1" ]; then
        start_spinner "Installing sdl library"
        owl install sdl 2>/dev/null || true
        stop_spinner 0 "sdl installed"
    fi
    if [ "$INSTALL_SQLITE" = "1" ]; then
        start_spinner "Installing sqlite library"
        owl install sqlite 2>/dev/null || true
        stop_spinner 0 "sqlite installed"
    fi
    if [ "$INSTALL_WASM" = "1" ]; then
        start_spinner "Installing wasm library"
        owl install wasm 2>/dev/null || true
        stop_spinner 0 "wasm installed"
    fi
    if [ "$MIREUP" = "1" ]; then
        start_spinner "Installing mire stdlib"
        owl install mire 2>/dev/null || true
        stop_spinner 0 "mire stdlib installed"
    fi
}

# Function to install libraries directly (fallback)
install_libraries_direct() {
    echo "Installing libraries to $OWL_HOME/libs..."
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
                echo "export PATH=\"$BIN_DIR:\$PATH\"" >> "$rc"
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
    echo -e "${BOLD}${YELLOW}=== Building from source ===${NC}"
    echo ""
    install_deps "build"
    install_rust
    
    TMPDIR_SRC=$(mktemp -d)
    cd "$TMPDIR_SRC"
    
    echo "Cloning repositories..."
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
        echo ""
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
        echo ""
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
    
    echo ""
    if [ "$INSTALL_COMPILER" = "1" ] || [ "$MIREUP" = "1" ]; then
        install_mire "$MIRE_VERSION"
    fi
    
    if [ "$INSTALL_OWL" = "1" ] || [ "$MIREUP" = "1" ]; then
        install_owl "$OWL_VERSION"
    fi
    
    # Install libraries
    if command -v owl >/dev/null 2>&1; then
        install_libraries
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
echo ""
echo -e "${BOLD}${BLUE}================================================${NC}"
echo -e "${BOLD}${BLUE}  Installation Complete!${NC}"
echo -e "${BOLD}${BLUE}================================================${NC}"

if [ -f "$BIN_DIR/owl" ]; then
    OWL_VER=$("$BIN_DIR/owl" -V 2>/dev/null | head -1 || echo "installed")
    log_success "owl: $OWL_VER"
fi
if [ -f "$BIN_DIR/mire" ]; then
    MIRE_VER=$("$BIN_DIR/mire" --version 2>/dev/null | head -1 || echo "installed")
    log_success "mire: $MIRE_VER"
fi

echo ""
echo -e "${CYAN}Next steps:${NC}"
echo "  1. Restart your shell or run: export PATH=\$PATH:$BIN_DIR"
echo "  2. Test: mire --version && owl --version"
echo "  3. Install libraries: owl install mire kioto ilogical"
echo ""
if [ "$UPDATING" = "1" ]; then
    log_info "Updated existing installation"
else
    log_info "Fresh installation complete"
fi
echo ""
#!/bin/bash
set -e

# Mire Toolchain Installer
# 
# First command: quick install via mireup
# curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --mireup
#
# Or install manually with options

SCRIPT_URL="${MIRE_INSTALL_SCRIPT_URL:-https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh}"
MIREUP_URL="${MIREUP_URL:-${SCRIPT_URL}}"

# Detect if running as root or user
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

usage() {
    cat << 'USAGE'
Mire Toolchain Installer

Usage:
  install.sh [options]

Options:
  --mireup              Quick default install (owl + kioto + mire)
  --interactive         Interactive menu to select components
  --from-source         Build everything from source
  --compiler            Also install Mire compiler
  --prefix <path>       Install prefix
  --yes, -y             Non-interactive
  --help, -h            Show this help

Examples:
  # Default quick install
  curl -fsSL .../install.sh | sh -s -- --mireup
  
  # Interactive selection
  curl -fsSL .../install.sh | sh -s -- --interactive
  
  # From source
  curl -fsSL .../install.sh | sh -s -- --from-source
USAGE
}

MIREUP=0
INTERACTIVE=0
FROM_SOURCE=0
INSTALL_COMPILER=0
INSTALL_OWL=1
INSTALL_KIOTO=1
YES=0

while [ $# -gt 0 ]; do
    case "$1" in
        --mireup) MIREUP=1; INSTALL_COMPILER=1; INSTALL_OWL=1; INSTALL_KIOTO=1; shift ;;
        --interactive) INTERACTIVE=1; shift ;;
        --from-source) FROM_SOURCE=1; shift ;;
        --compiler) INSTALL_COMPILER=1; shift ;;
        --prefix) PREFIX="$2"; BIN_DIR="${PREFIX}/bin"; shift 2 ;;
        --yes|-y) YES=1; shift ;;
        --help|-h) usage; exit 0 ;;
        *) shift ;;
    esac
done

echo "=============================================="
echo "  Mire Toolchain Installer"
echo "=============================================="
echo "Prefix: $PREFIX"
echo "User: $(whoami)"
echo ""

# Ensure directories
mkdir -p "$BIN_DIR" "$OWL_HOME" "$OWL_HOME/libs" "$OWL_HOME/modules" "$MIRE_HOME"

if [ "$INTERACTIVE" = "1" ]; then
    echo "Select what to install (space separated or comma separated):"
    echo "  1) Avenys (mire compiler)"
    echo "  2) owl (package manager)"
    echo "  3) kioto (library)"
    echo "  4) sdl (library)"
    echo "  5) sqlite (library)"
    echo "  6) wasm (library)"
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

if [ "$FROM_SOURCE" = "1" ]; then
    echo "=== Building from source ==="
    echo "Installing dependencies..."
    if command -v apt-get >/dev/null 2>&1; then
        if [ "$IS_ROOT" = "1" ]; then
            apt-get update -qq && apt-get install -y -qq curl git build-essential clang llvm lld pkg-config libssl-dev libsodium-dev zlib1g-dev libzstd-dev libarchive-dev tcc
        else
            sudo apt-get update -qq && sudo apt-get install -y -qq curl git build-essential clang llvm lld pkg-config libssl-dev libsodium-dev zlib1g-dev libzstd-dev libarchive-dev tcc
        fi
    elif command -v pacman >/dev/null 2>&1; then
        if [ "$IS_ROOT" = "1" ]; then
            pacman -S --needed --noconfirm curl git base-devel clang llvm lld openssl libsodium zlib zstd libarchive tcc
        else
            sudo pacman -S --needed --noconfirm curl git base-devel clang llvm lld openssl libsodium zlib zstd libarchive tcc
        fi
    fi
    
    # Install Rust if needed
    if ! command -v rustc >/dev/null 2>&1; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
        export PATH="$HOME/.cargo/bin:$PATH"
    fi
    
    TMPDIR_SRC=$(mktemp -d)
    cd "$TMPDIR_SRC"
    
    echo "Cloning repositories..."
    git clone --depth 1 https://github.com/mire-lang/Avenys-rust avenys 2>/dev/null || true
    git clone --depth 1 https://github.com/mire-lang/owl owl 2>/dev/null || true
    git clone --depth 1 https://github.com/mire-lang/Kioto kioto 2>/dev/null || true
    git clone --depth 1 https://github.com/mire-lang/mire-lib mire-lib 2>/dev/null || true
    
    if [ -d avenys ]; then
        echo "Building mire compiler..."
        cd avenys
        LLVM_CONFIG_PATH=$(which llvm-config-22 2>/dev/null || which llvm-config 2>/dev/null) cargo build --release
        if [ -f target/release/mire ]; then
            cp target/release/mire "$BIN_DIR/mire"
            chmod +x "$BIN_DIR/mire"
            mkdir -p "$PREFIX/lib/mire"
            cp -r src/runtime "$PREFIX/lib/mire/" 2>/dev/null || true
            cp -r src/pal "$PREFIX/lib/mire/" 2>/dev/null || true
            echo "mire installed"
        fi
        cd ..
    fi
    
    if [ -d owl ]; then
        echo "Building owl..."
        export PATH="$BIN_DIR:$PATH"
        if command -v mire >/dev/null 2>&1; then
            cd owl
            mkdir -p "$OWL_HOME/libs/kioto" "$OWL_HOME/libs/mire"
            cp -r ../kioto/* "$OWL_HOME/libs/kioto/" 2>/dev/null || true
            cp -r ../mire-lib/* "$OWL_HOME/libs/mire/" 2>/dev/null || true
            cat > native/mire-config.toml << CFGEOF
[project]
name = "owl"
version = "1.3.0"
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
                echo "owl installed"
            fi
            cd ..
        fi
    fi
    
    rm -rf "$TMPDIR_SRC"
else
    # Download from releases
    ARCH=$(uname -m)
    case "$ARCH" in
        x86_64) TRIPLE="x86_64-unknown-linux-gnu" ;;
        aarch64) TRIPLE="aarch64-unknown-linux-gnu" ;;
        *) TRIPLE="x86_64-unknown-linux-gnu" ;;
    esac
    
    if [ "$INSTALL_OWL" = "1" ] && [ "$MIREUP" = "1" -o -z "$(command -v owl 2>/dev/null)" ]; then
        echo "Installing owl..."
        LATEST_OWL=$(curl -s "https://api.github.com/repos/mire-lang/owl/releases/latest" 2>/dev/null | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/' | head -1)
        if [ -n "$LATEST_OWL" ]; then
            curl -fsSL "https://github.com/mire-lang/owl/releases/download/${LATEST_OWL}/owl-${TRIPLE}.tar.gz" -o /tmp/owl.tar.gz 2>/dev/null || true
            if [ -f /tmp/owl.tar.gz ]; then
                tar xzf /tmp/owl.tar.gz -C /tmp
                if [ -f /tmp/owl/owl ]; then
                    cp /tmp/owl/owl "$BIN_DIR/owl"
                    chmod +x "$BIN_DIR/owl"
                    [ -d /tmp/owl/lib ] && mkdir -p "$PREFIX/lib/mire" && cp /tmp/owl/lib/* "$PREFIX/lib/mire/" 2>/dev/null || true
                    [ -d /tmp/owl/kioto ] && cp -r /tmp/owl/kioto "$OWL_HOME/libs/" 2>/dev/null || true
                    echo "owl installed"
                fi
                rm -rf /tmp/owl /tmp/owl.tar.gz
            fi
        fi
    fi
    
    if [ "$INSTALL_COMPILER" = "1" ]; then
        echo "Installing mire compiler..."
        LATEST_COMP=$(curl -s "https://api.github.com/repos/mire-lang/Avenys-rust/releases/latest" 2>/dev/null | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/' | head -1)
        if [ -n "$LATEST_COMP" ]; then
            curl -fsSL "https://github.com/mire-lang/Avenys-rust/releases/download/${LATEST_COMP}/mire-compiler-${TRIPLE}.tar.gz" -o /tmp/mire.tar.gz 2>/dev/null || true
            if [ -f /tmp/mire.tar.gz ]; then
                tar xzf /tmp/mire.tar.gz -C /tmp
                if [ -f /tmp/compiler/mire ]; then
                    cp /tmp/compiler/mire "$BIN_DIR/mire"
                    chmod +x "$BIN_DIR/mire"
                    mkdir -p "$PREFIX/lib/mire"
                    [ -d /tmp/compiler/runtime ] && cp -r /tmp/compiler/runtime "$PREFIX/lib/mire/"
                    [ -d /tmp/compiler/pal ] && cp -r /tmp/compiler/pal "$PREFIX/lib/mire/"
                    echo "mire installed"
                fi
                rm -rf /tmp/compiler /tmp/mire.tar.gz
            fi
        fi
    fi
fi

# Install libraries via owl if available
if command -v owl >/dev/null 2>&1; then
    export PATH="$BIN_DIR:$PATH"
    echo ""
    echo "Installing libraries..."
    # Ensure MOR registry
    owl reg 2>/dev/null || true
    if [ "$INSTALL_KIOTO" = "1" ]; then owl install kioto 2>/dev/null || true; fi
    if [ "$INSTALL_SDL" = "1" ]; then owl install sdl 2>/dev/null || true; fi
    if [ "$INSTALL_SQLITE" = "1" ]; then owl install sqlite 2>/dev/null || true; fi
    if [ "$INSTALL_WASM" = "1" ]; then owl install wasm 2>/dev/null || true; fi
    # Default install mire and kioto
    if [ "$MIREUP" = "1" ]; then
        owl install mire 2>/dev/null || true
        owl install kioto 2>/dev/null || true
    fi
else
    # Install libraries directly
    echo "Installing libraries to $OWL_HOME/libs..."
    mkdir -p "$OWL_HOME/libs"
    for lib in kioto mire sdl sqlite wasm; do
        if [ ! -d "$OWL_HOME/libs/$lib" ] && [ ! -d "$OWL_HOME/libs/$lib@*" ]; then
            case "$lib" in
                kioto)
                    curl -fsSL "https://github.com/mire-lang/Kioto/archive/refs/heads/master.tar.gz" -o /tmp/kioto.tar.gz 2>/dev/null && \
                    tar xzf /tmp/kioto.tar.gz -C /tmp && \
                    for d in /tmp/Kioto-*/; do [ -f "$d/owl.toml" ] && cp -r "$d" "$OWL_HOME/libs/kioto" && break; done && rm -rf /tmp/kioto.tar.gz /tmp/Kioto-* 2>/dev/null || true
                    ;;
                mire)
                    curl -fsSL "https://github.com/mire-lang/mire-lib/archive/refs/heads/main.tar.gz" -o /tmp/mire.tar.gz 2>/dev/null && \
                    tar xzf /tmp/mire.tar.gz -C /tmp && \
                    for d in /tmp/mire-lib-*/; do [ -f "$d/owl.toml" ] || true; done && \
                    # Find the right dir
                    for d in /tmp/mire-lib-*/; do cp -r "$d" "$OWL_HOME/libs/mire" 2>/dev/null && break; done && rm -rf /tmp/mire.tar.gz /tmp/mire-lib-* 2>/dev/null || true
                    ;;
            esac
        fi
    done
fi

# Add to PATH
if [ "$NO_PROFILE" != "1" ]; then
    for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
        if [ -f "$rc" ] && ! grep -q "$BIN_DIR" "$rc"; then
            echo "export PATH=\"$BIN_DIR:\$PATH\"" >> "$rc"
            echo "Added $BIN_DIR to $rc"
            break
        fi
    done
fi

echo ""
echo "=============================================="
echo "  Installation complete!"
echo "=============================================="
if [ -f "$BIN_DIR/owl" ]; then echo "  owl: $(file $BIN_DIR/owl 2>/dev/null | grep -o BuildID.* || echo installed)"; fi
if [ -f "$BIN_DIR/mire" ]; then echo "  mire: $(file $BIN_DIR/mire 2>/dev/null | grep -o BuildID.* || echo installed)"; fi
echo ""
echo "Restart your shell or run: export PATH=$BIN_DIR:\$PATH"
echo ""

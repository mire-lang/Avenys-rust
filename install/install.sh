#!/bin/bash
set -e

# Mire Toolchain Installer
#
# ============================================================
# SEPARATE INSTALLATION LINKS (copy/paste each as needed)
# ============================================================
#
# 1) QUICK INSTALL (mire + owl + kioto):
#    curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --mireup
#
# 2) INSTALL MIRE COMPILER ONLY:
#    curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --compiler
#
# 3) INSTALL OWL PACKAGE MANAGER ONLY:
#    curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --owl
#
# 4) INSTALL KIOTO LIBRARY ONLY:
#    curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --kioto
#
# 5) INSTALL SDL LIBRARY ONLY:
#    curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --sdl
#
# 6) INSTALL SQLITE LIBRARY ONLY:
#    curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --sqlite
#
# 7) INSTALL WASM LIBRARY ONLY:
#    curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --wasm
#
# 8) FULL INTERACTIVE INSTALL (select components):
#    curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --interactive
#
# 9) BUILD FROM SOURCE (all components):
#    curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --from-source
#
# 10) UNINSTALL:
#     curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | sh
#
# ============================================================
# DIRECT RELEASE DOWNLOAD LINKS (for manual installation)
# ============================================================
#
# MIRE COMPILER (latest release):
#   x86_64:   curl -fsSL https://github.com/mire-lang/Avenys-rust/releases/latest/download/mire-compiler-x86_64-unknown-linux-gnu.tar.gz
#   aarch64:  curl -fsSL https://github.com/mire-lang/Avenys-rust/releases/latest/download/mire-compiler-aarch64-unknown-linux-gnu.tar.gz
#   riscv64:  curl -fsSL https://github.com/mire-lang/Avenys-rust/releases/latest/download/mire-compiler-riscv64-unknown-linux-gnu.tar.gz
#
# OWL PACKAGE MANAGER (latest release):
#   x86_64:   curl -fsSL https://github.com/mire-lang/owl/releases/latest/download/owl-linux-x86_64.tar.gz
#   aarch64:  curl -fsSL https://github.com/mire-lang/owl/releases/latest/download/owl-linux-aarch64.tar.gz
#
# LIBRARIES (via owl after installation):
#   owl install mire       # mire stdlib (v2.1.0-beta)
#   owl install kioto      # kioto stdlib
#   owl install sdl        # SDL bindings
#   owl install sqlite     # SQLite bindings
#   owl install wasm       # WASM support
#   owl install ilogical   # ilogical library
#
# ============================================================
# END OF SEPARATE LINKS SECTION
# ============================================================

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
  --yes, -y             Non-interactive
  --help, -h            Show this help

Examples:
  # Quick install (mire + owl + kioto)
  curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --mireup

  # Install specific components
  curl -fsSL .../install.sh | sh -s -- --compiler --owl --kioto

  # Interactive selection
  curl -fsSL .../install.sh | sh -s -- --interactive

  # Build from source
  curl -fsSL .../install.sh | sh -s -- --from-source
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

echo "=============================================="
echo "  Mire Toolchain Installer"
echo "=============================================="
echo "Prefix: $PREFIX"
echo "User: $(whoami)"
echo ""

# Ensure directories
mkdir -p "$BIN_DIR" "$OWL_HOME" "$OWL_HOME/libs" "$OWL_HOME/modules" "$MIRE_HOME"
mkdir -p "$PREFIX/lib/mire"

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

# Function to fix binary RPATH for bundled libraries
fix_binary_rpath() {
    local binary="$1"
    local lib_dir="$2"

    # Calculate relative path from binary to lib directory
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

if [ "$FROM_SOURCE" = "1" ]; then
    echo "=== Building from source ==="
    echo "Installing dependencies..."
    if command -v apt-get >/dev/null 2>&1; then
        if [ "$IS_ROOT" = "1" ]; then
            apt-get update -qq && apt-get install -y -qq curl git build-essential clang llvm lld pkg-config libssl-dev libsodium-dev zlib1g-dev libzstd-dev libarchive-dev tcc patchelf
        else
            sudo apt-get update -qq && sudo apt-get install -y -qq curl git build-essential clang llvm lld pkg-config libssl-dev libsodium-dev zlib1g-dev libzstd-dev libarchive-dev tcc patchelf
        fi
    elif command -v pacman >/dev/null 2>&1; then
        if [ "$IS_ROOT" = "1" ]; then
            pacman -S --needed --noconfirm curl git base-devel clang llvm lld openssl libsodium zlib zstd libarchive tcc patchelf
        else
            sudo pacman -S --needed --noconfirm curl git base-devel clang llvm lld openssl libsodium zlib zstd libarchive tcc patchelf
        fi
    elif command -v dnf >/dev/null 2>&1; then
        if [ "$IS_ROOT" = "1" ]; then
            dnf install -y curl git gcc gcc-c++ clang llvm llvm-devel lld cargo pkgconf-pkg-config openssl-devel libsodium-devel zlib-devel zstd-devel libarchive-devel patchelf
        else
            sudo dnf install -y curl git gcc gcc-c++ clang llvm llvm-devel lld cargo pkgconf-pkg-config openssl-devel libsodium-devel zlib-devel zstd-devel libarchive-devel patchelf
        fi
    elif command -v zypper >/dev/null 2>&1; then
        if [ "$IS_ROOT" = "1" ]; then
            zypper install -y curl git gcc gcc-c++ clang llvm llvm-devel lld rust cargo libopenssl-devel libsodium-devel zlib-devel libzstd-devel libarchive-devel patchelf
        else
            sudo zypper install -y curl git gcc gcc-c++ clang llvm llvm-devel lld rust cargo libopenssl-devel libsodium-devel zlib-devel libzstd-devel libarchive-devel patchelf
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
version = "1.6.2"
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

    # Install owl package manager
    if [ "$INSTALL_OWL" = "1" ] || [ "$MIREUP" = "1" ]; then
        echo "Installing owl package manager..."
        LATEST_OWL=$(curl -s "https://api.github.com/repos/mire-lang/owl/releases/latest" 2>/dev/null | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/' | head -1)
        if [ -n "$LATEST_OWL" ]; then
            curl -fsSL "https://github.com/mire-lang/owl/releases/download/${LATEST_OWL}/owl-${TRIPLE}.tar.gz" -o /tmp/owl.tar.gz 2>/dev/null || true
            if [ -f /tmp/owl.tar.gz ]; then
                tar xzf /tmp/owl.tar.gz -C /tmp
                if [ -f /tmp/owl/owl ]; then
                    cp /tmp/owl/owl "$BIN_DIR/owl"
                    chmod +x "$BIN_DIR/owl"
                    [ -d /tmp/owl/lib ] && cp /tmp/owl/lib/* "$PREFIX/lib/mire/" 2>/dev/null || true
                    [ -d /tmp/owl/kioto ] && cp -r /tmp/owl/kioto "$OWL_HOME/libs/" 2>/dev/null || true
                    fix_binary_rpath "$BIN_DIR/owl" "$PREFIX/lib/mire"
                    echo "owl installed (${LATEST_OWL})"
                fi
                rm -rf /tmp/owl /tmp/owl.tar.gz
            fi
        fi
    fi

    # Install mire compiler
    if [ "$INSTALL_COMPILER" = "1" ] || [ "$MIREUP" = "1" ]; then
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
                    echo "mire installed (${LATEST_COMP})"
                fi
                rm -rf /tmp/compiler /tmp/mire.tar.gz
            fi
        fi
    fi

    # Install tcc for C object compilation (required by mire compiler)
    if ! command -v tcc >/dev/null 2>&1; then
        echo "Installing tcc (required for C object compilation)..."
        TCC_INSTALLED=0
        if [ "$IS_ROOT" = "1" ]; then
            if command -v apt-get >/dev/null 2>&1; then
                apt-get update -qq && apt-get install -y -qq tcc && TCC_INSTALLED=1
            elif command -v pacman >/dev/null 2>&1; then
                pacman -S --needed --noconfirm tcc && TCC_INSTALLED=1
            elif command -v dnf >/dev/null 2>&1; then
                dnf install -y tcc && TCC_INSTALLED=1
            elif command -v zypper >/dev/null 2>&1; then
                zypper install -y tcc && TCC_INSTALLED=1
            fi
        else
            if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
                if command -v apt-get >/dev/null 2>&1; then
                    sudo apt-get update -qq && sudo apt-get install -y -qq tcc && TCC_INSTALLED=1
                elif command -v pacman >/dev/null 2>&1; then
                    sudo pacman -S --needed --noconfirm tcc && TCC_INSTALLED=1
                elif command -v dnf >/dev/null 2>&1; then
                    sudo dnf install -y tcc && TCC_INSTALLED=1
                elif command -v zypper >/dev/null 2>&1; then
                    sudo zypper install -y tcc && TCC_INSTALLED=1
                fi
            fi
        fi
        if [ "$TCC_INSTALLED" != "1" ]; then
            echo "WARNING: tcc not installed. The mire compiler requires tcc for C object compilation."
            echo "         Install tcc manually (e.g., 'sudo apt-get install tcc') or use --from-source to install it automatically."
        fi
    fi
fi

# Install libraries via owl if available
if command -v owl >/dev/null 2>&1; then
    export PATH="$BIN_DIR:$PATH"
    echo ""
    echo "Installing libraries via owl..."
    # Ensure MOR registry
    owl reg 2>/dev/null || true
    if [ "$INSTALL_KIOTO" = "1" ] || [ "$MIREUP" = "1" ]; then owl install kioto 2>/dev/null || true; fi
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
if [ -f "$BIN_DIR/owl" ]; then echo "  owl: installed"; fi
if [ -f "$BIN_DIR/mire" ]; then echo "  mire: installed"; fi
echo ""
echo "Restart your shell or run: export PATH=$BIN_DIR:\$PATH"
echo ""
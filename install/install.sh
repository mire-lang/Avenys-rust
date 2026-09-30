#!/bin/sh
set -e

# ── Mire Toolchain Install ────────────────────────────────────────────
#
# Usage:
#   curl -fsSL <url> | sh                        # install Owl + Kioto (default)
#   curl -fsSL <url> | sh -s -- --compiler       # also install mire compiler
#   curl -fsSL <url> | sh -s -- --compiler-only   # compiler only
#   curl -fsSL <url> | sh -s -- --kioto-only      # kioto stdlib only
#   curl -fsSL <url> | sh -s -- --build-from-source  # build the compiler locally
#
# Release architectures (auto-detected from `uname -m`, overridable):
#   x86_64-unknown-linux-gnu   (amd64)   default release asset
#   aarch64-unknown-linux-gnu  (arm64)
#   riscv64-unknown-linux-gnu  (riscv64)
# Assets are named `mire-compiler-<target>.tar.gz` / `owl-<target>.tar.gz`,
# with a fallback to the legacy `mire-compiler-linux-x86_64.tar.gz` naming.
#
# The installer downloads a prebuilt release. Prebuilt binaries require:
#   • GNU glibc >= 2.39
#   • LLVM/Clang 18-22 (any distribution-native version works: mire lowers
#     plain-text LLVM IR, so distro `opt`/`llc`/`lld`/`clang` are used as-is)
#   • openssl / libsodium / zlib / zstd / libarchive development libraries
#
# Distributions below the glibc floor (Ubuntu <24.04, Debian <13,
# Fedora <41, RHEL/Oracle/… <10, musl/Alpine) print
#     "This distro <distro> is not supported by mire-lang"
# and offer a Docker container shipping a modern glibc instead. Set
# MIRE_SKIP_DISTRO_CHECK=1 to bypass the safety check.
#
# `--build-from-source` skips the prebuilt compiler and instead verifies Rust,
# (optionally) installs/updates it via rustup, fetches
# https://github.com/mire-lang/Avenys-rust and runs `cargo build --release`.
#
# Hermetic / mirror support (also used by the CI test harness):
#   MIRE_BASE_URL   default https://github.com        (downloads)
#   MIRE_API_URL    default https://api.github.com    (latest-tag lookup)
#   MIRE_KIOTO_URL  default https://github.com/<repo> (git clone URL)

REPO_OWL="${OWL_REPO:-Evelynx-Dev/owl}"
REPO_COMPILER="${COMPILER_REPO:-Evelynx-Dev/Avenys-rust}"
REPO_KIOTO="${KIOTO_REPO:-Evelynx-Dev/Kioto-1}"
REPO_MIRE="${MIRE_REPO:-mire-lang/mire-lib}"
BASE_URL="${MIRE_BASE_URL:-https://github.com}"
API_URL="${MIRE_API_URL:-https://api.github.com}"
KIOTO_URL="${MIRE_KIOTO_URL:-https://github.com/${REPO_KIOTO}}"

# Per-architecture release assets. The canonical names are
#   mire-compiler-<target>.tar.gz / owl-<target>.tar.gz
# with a legacy fallback (Owl_TARBALL_LEGACY) for older x86_64 releases that
# used `mire-compiler-linux-x86_64.tar.gz`. Set OWL_TARBALL / COMPILER_TARBALL
# to force a specific asset name.
ARCH_TARGET="${MIRE_ARCH:-}"
HOST_ARCH=""
TARGET=""
OWL_TARBALL="${OWL_TARBALL:-}"
COMPILER_TARBALL="${COMPILER_TARBALL:-}"
OWL_TARBALL_LEGACY="owl-linux-x86_64.tar.gz"
COMPILER_TARBALL_LEGACY="mire-compiler-linux-x86_64.tar.gz"

# `--build-from-source`: compile the Avenys compiler locally instead of
# downloading the prebuilt release archive.
BUILD_FROM_SOURCE="${MIRE_BUILD_FROM_SOURCE:-0}"
SOURCE_URL="${MIRE_SOURCE_URL:-https://github.com/mire-lang/avenys-rust}"
SOURCE_REF="${MIRE_SOURCE_REF:-main}"
MIN_RUST_VERSION="1.85"   # edition-2024 floor (rustc/cargo minimum)
RUSTUP_URL="${MIRE_RUSTUP_URL:-https://sh.rustup.rs}"

# Docker fallback for distros below the minimum glibc (or when --docker).
DOCKER_IMAGE="${MIRE_DOCKER_IMAGE:-mire-lang/toolchain:${ARCH_TARGET:-latest}}"
RUN_DOCKER=0

PREFIX=""
TAG_COMPILER=""
TAG_KIOTO=""
YES=0
NO_PROFILE=0
CHECK_ONLY=0
INSTALL_COMPILER=0
INSTALL_OWL=1
INSTALL_KIOTO=1
COMPILER_ONLY=0
KIOTO_ONLY=0

DISTRO_ID=""
DISTRO_ID_LIKE=""
DISTRO_VERSION=""
DISTRO_PRETTY=""
DISTRO_FAMILY=""
GLIBC_VERSION=""
LIBC_MUSL=0

usage() {
    cat <<'USAGE'
Mire Toolchain Install

Usage:
  install.sh [options]

Options:
  --yes, -y             Non-interactive (skip confirmations)
  --prefix <path>        Install prefix (default: /usr/local)
  --compiler             Also install the Mire compiler
  --compiler-only        Install compiler only
  --kioto-only           Install kioto stdlib only (sets up ~/.owl/)
  --owl-only             Install owl only (no compiler, no kioto)
  --arch <target>        Release target triple (e.g. aarch64-unknown-linux-gnu)
  --build-from-source    Build the compiler from source (rust + cargo, no tarball)
  --docker               Run the toolchain inside the provided container instead
  --no-profile           Skip shell profile PATH modification
  --check                Check prerequisites only; never install packages
  --tag-compiler <tag>   Specific release tag (default: latest)
  --tag-kioto <tag>      Specific kioto release tag (default: latest)
  --help, -h             Show this help

Examples:
  curl https://.../install.sh | sh                     # install owl + kioto (default)
  curl https://.../install.sh | sh -s -- --compiler     # install owl + kioto + compiler
  curl https://.../install.sh | sh -s -- --compiler-only # compiler only
  curl https://.../install.sh | sh -s -- --kioto-only    # kioto only
  curl https://.../install.sh | sh -s -- --arch aarch64-unknown-linux-gnu
  curl https://.../install.sh | sh -s -- --build-from-source --compiler
  ./install.sh --check                                 # audit the host (no changes)

Interactive installs ask for confirmation before refreshing package
indexes, installing packages, and modifying your shell profile. Use
--yes for a fully non-interactive run.

Documentation:
  https://github.com/mire-lang/Avenys-rust#readme
  https://mire-lang.github.io
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
        --yes|-y)             YES=1; shift ;;
        --prefix)              PREFIX="$2"; shift 2 ;;
        --compiler)            INSTALL_COMPILER=1; shift ;;
        --compiler-only)        COMPILER_ONLY=1; INSTALL_OWL=0; INSTALL_KIOTO=0; shift ;;
        --kioto-only)           KIOTO_ONLY=1; INSTALL_COMPILER=0; INSTALL_OWL=0; shift ;;
        --owl-only)             INSTALL_OWL=1; INSTALL_KIOTO=0; INSTALL_COMPILER=0; shift ;;
        --arch)                 ARCH_TARGET="$2"; shift 2 ;;
        --build-from-source|--source) BUILD_FROM_SOURCE=1; INSTALL_COMPILER=1; shift ;;
        --docker)               RUN_DOCKER=1; shift ;;
        --no-profile)          NO_PROFILE=1; shift ;;
        --check)               CHECK_ONLY=1; shift ;;
        --tag-owl)              TAG_COMPILER="$2"; shift 2 ;;
        --tag-compiler)         TAG_COMPILER="$2"; shift 2 ;;
        --tag-kioto)            TAG_KIOTO="$2"; shift 2 ;;
        --help|-h)              usage; exit 0 ;;
        --)                     shift; break ;;
        -*)                     echo "error: unknown option: $1" >&2; usage; exit 1 ;;
        *)                      echo "error: unexpected argument: $1" >&2; usage; exit 1 ;;
    esac
done

if [ -z "$PREFIX" ]; then
    PREFIX="${MIRE_PREFIX:-/usr/local}"
fi

case "$PREFIX" in
    ~/*) PREFIX="${HOME}${PREFIX#~}" ;;
    ~)   PREFIX="${HOME}" ;;
esac

BIN_DIR="${PREFIX}/bin"
LIB_DIR="${PREFIX}/lib/mire"
OWL_HOME="$HOME/.owl"

banner() {
    echo ""
    echo "┌─ Mire Toolchain Install ─────────────────────────────────────────┐"
    echo "│ prefix: ${PREFIX}"
    if [ "$KIOTO_ONLY" = "1" ]; then
        echo "│ mode:   kioto only (stdlib)"
    elif [ "$COMPILER_ONLY" = "1" ]; then
        echo "│ mode:   compiler only"
    elif [ "$INSTALL_COMPILER" = "1" ]; then
        echo "│ mode:   owl + kioto + compiler (full)"
    elif [ "$INSTALL_OWL" = "1" ] && [ "$INSTALL_KIOTO" = "1" ]; then
        echo "│ mode:   owl + kioto (default)"
    elif [ "$INSTALL_OWL" = "1" ]; then
        echo "│ mode:   owl only"
    else
        echo "│ mode:   compiler only"
    fi
    echo "└──────────────────────────────────────────────────────────────────┘"
}

banner

# ── Architecture ------------------------------------------------------
detect_arch() {
    local m
    m="$(uname -m 2>/dev/null || echo unknown)"
    case "$m" in
        x86_64|amd64)        echo "x86_64-unknown-linux-gnu" ;;
        aarch64|arm64)       echo "aarch64-unknown-linux-gnu" ;;
        riscv64|riscv64gc)   echo "riscv64-unknown-linux-gnu" ;;
        *)                   echo "unsupported-${m}" ;;
    esac
}

arch_supported() {
    case "$TARGET" in
        x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|riscv64-unknown-linux-gnu)
            return 0 ;;
        *) return 1 ;;
    esac
}

# Interactive target confirmation. Non-interactive (no tty) or --yes keeps
# the detected architecture so `curl … | sh` never blocks.
select_arch() {
    if [ "$YES" = "1" ] || [ ! -t 0 ]; then
        echo "  target: ${TARGET}"
        return 0
    fi
    printf '  use architecture %s (detected)? [Y/n] ' "$TARGET"
    ans=""
    read -r ans || ans=""
    case "$ans" in
        [yY]*|"")
            echo "  target: ${TARGET}"
            return 0
            ;;
        [nN]*)
            echo ""
            echo "  Available release architectures:"
            echo "    1) x86_64-unknown-linux-gnu   (amd64)"
            echo "    2) aarch64-unknown-linux-gnu  (arm64)"
            echo "    3) riscv64-unknown-linux-gnu  (riscv64)"
            echo "    (cross-architecture binaries only run if your kernel"
            echo "     provides instruction-set support, e.g. qemu-user.)"
            while :; do
                printf '  select [1-3] (empty = keep %s, q = cancel): ' "$TARGET"
                sel=""
                read -r sel || sel="q"
                case "$sel" in
                    1) TARGET="x86_64-unknown-linux-gnu"; break ;;
                    2) TARGET="aarch64-unknown-linux-gnu"; break ;;
                    3) TARGET="riscv64-unknown-linux-gnu"; break ;;
                    ""|"") break ;;
                    q|Q) echo "  aborted."; exit 0 ;;
                    *)   echo "  invalid choice: ${sel}" ;;
                esac
            done
            echo "  target: ${TARGET}"
            ;;
        *) echo "  assuming yes: ${TARGET}" ;;
    esac
}

# Download the first URL that returns a file. Prints "$dest" on success,
# nothing on failure (mirrors the old curl-then-wget fallback, but tries a
# whole candidate list so canonical + legacy asset names both work).
fetch_first() {
    local dest="$1"; shift
    local ok="" u
    for u in "$@"; do
        if command -v curl >/dev/null 2>&1; then
            if curl -fsSL "$u" -o "$dest" 2>/dev/null; then ok=1; break; fi
        elif command -v wget >/dev/null 2>&1; then
            if wget -q "$u" -O "$dest" 2>/dev/null; then ok=1; break; fi
        fi
    done
    [ -n "$ok" ] && echo "$dest"
}

# Run a command with root privileges. Works when already root (containers,
# dedicated servers) as well as through sudo/su.
as_root() {
    if [ "$(id -u)" = "0" ]; then
        "$@"
    elif command -v sudo >/dev/null 2>&1; then
        sudo "$@"
    else
        "$@"
    fi
}

needs_sudo() {
    case "$PREFIX" in
        /usr|/usr/local|/opt*|/etc*) return 0 ;;
        *) return 1 ;;
    esac
}

detect_pkg_manager() {
    if command -v apt-get >/dev/null 2>&1; then
        echo "apt"
    elif command -v pacman >/dev/null 2>&1; then
        echo "pacman"
    elif command -v dnf >/dev/null 2>&1; then
        echo "dnf"
    elif command -v yum >/dev/null 2>&1; then
        echo "yum"
    elif command -v apk >/dev/null 2>&1; then
        echo "apk"
    elif command -v zypper >/dev/null 2>&1; then
        echo "zypper"
    else
        echo "none"
    fi
}

# Read /etc/os-release without eval (safe for untrusted content).
parse_os_release() {
    DISTRO_ID="$(sed -n 's/^ID=//p' /etc/os-release 2>/dev/null | head -1 | tr -d '"')"
    DISTRO_ID_LIKE="$(sed -n 's/^ID_LIKE=//p' /etc/os-release 2>/dev/null | head -1 | tr -d '"')"
    DISTRO_VERSION="$(sed -n 's/^VERSION_ID=//p' /etc/os-release 2>/dev/null | head -1 | tr -d '"')"
    DISTRO_PRETTY="$(sed -n 's/^PRETTY_NAME=//p' /etc/os-release 2>/dev/null | head -1 | tr -d '"')"
    if [ -z "$DISTRO_PRETTY" ]; then
        DISTRO_PRETTY="${DISTRO_ID} ${DISTRO_VERSION}"
    fi
}

detect_distro() {
    if [ -r /etc/os-release ]; then
        parse_os_release
    elif [ -r /etc/redhat-release ]; then
        DISTRO_ID="rhel"
        DISTRO_PRETTY="$(cat /etc/redhat-release 2>/dev/null)"
        DISTRO_VERSION="$(sed -n 's/.*release[^0-9]*\([0-9][0-9]*\).*/\1/p' /etc/redhat-release 2>/dev/null)"
    else
        DISTRO_ID="unknown"
        DISTRO_PRETTY="$(uname -s) $(uname -m)"
        DISTRO_VERSION=""
    fi
}

detect_glibc() {
    GLIBC_VERSION=""
    LIBC_MUSL=0
    if getconf GNU_LIBC_VERSION >/dev/null 2>&1; then
        set -- $(getconf GNU_LIBC_VERSION 2>/dev/null || :)
        GLIBC_VERSION="$2"
    fi
    if [ -z "$GLIBC_VERSION" ]; then
        if ldd --version 2>/dev/null | grep -qi musl; then
            LIBC_MUSL=1
        else
            GLIBC_VERSION="$(ldd --version 2>/dev/null | head -1 \
                | sed -n 's/.*\([0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' || :)"
        fi
    fi
}

distro_family() {
    case "$DISTRO_ID" in
        ubuntu|debian) echo "debian" ;;
        arch|archarm|manjaro|endeavouros|cachyos|artix|garuda) echo "arch" ;;
        fedora) echo "fedora" ;;
        ol|rhel|rocky|almalinux|centos|centos-stream|amzn) echo "redhat" ;;
        alpine|void) echo "alpine" ;;
        opensuse-leap|opensuse-tumbleweed|sles|suse) echo "suse" ;;
        *)
            case " $DISTRO_ID_LIKE " in
                *" debian "*) echo "debian" ;;
                *" rhel "*|*" fedora "*) echo "redhat" ;;
                *" arch "*) echo "arch" ;;
                *) echo "" ;;
            esac
    esac
}

# Numeric compare of "X.Y" versions. Returns 0 if $1 >= $2.
ver_ge() {
    a_major="${1%%.*}"; a_rest="${1#*.}"; a_minor="${a_rest%%.*}"
    b_major="${2%%.*}"; b_rest="${2#*.}"; b_minor="${b_rest%%.*}"
    a_major="${a_major:-0}"; a_minor="${a_minor:-0}"
    b_major="${b_major:-0}"; b_minor="${b_minor:-0}"
    if [ "${a_major}" -gt "${b_major}" ]; then return 0; fi
    if [ "${a_major}" -lt "${b_major}" ]; then return 1; fi
    if [ "${a_minor}" -ge "${b_minor}" ]; then return 0; fi
    return 1
}

# Sets DISTRO_REASON on failure. Returns 0 (supported) or 1 (unsupported).
check_distro_support() {
    DISTRO_REASON=""
    [ "$MIRE_SKIP_DISTRO_CHECK" = "1" ] && return 0

    # kioto-only is a plain source checkout — no prebuilt binary floor.
    if [ "$KIOTO_ONLY" = "1" ]; then
        return 0
    fi

    if [ "$LIBC_MUSL" = "1" ]; then
        DISTRO_REASON="musl libc is not supported: mire prebuilt binaries link GNU glibc"
        return 1
    fi
    if [ -n "$GLIBC_VERSION" ] && ! ver_ge "$GLIBC_VERSION" "2.39"; then
        DISTRO_REASON="glibc ${GLIBC_VERSION} is too old: mire prebuilt binaries require glibc >= 2.39"
        return 1
    fi

    case "$DISTRO_ID" in
        ubuntu)
            ver_ge "$DISTRO_VERSION" "24.04" \
                || { DISTRO_REASON="Ubuntu ${DISTRO_VERSION} ships glibc < 2.39"; return 1; } ;;
        debian)
            ver_ge "$DISTRO_VERSION" "13.0" \
                || { DISTRO_REASON="Debian ${DISTRO_VERSION} ships glibc < 2.39"; return 1; } ;;
        fedora)
            ver_ge "$DISTRO_VERSION" "41.0" \
                || { DISTRO_REASON="Fedora ${DISTRO_VERSION} ships glibc < 2.39"; return 1; } ;;
        ol|rhel|rocky|almalinux|centos|centos-stream)
            ver_ge "$DISTRO_VERSION" "10.0" \
                || { DISTRO_REASON="${DISTRO_ID} ${DISTRO_VERSION} ships glibc < 2.39"; return 1; } ;;
        amzn)
            DISTRO_REASON="Amazon Linux 2023 ships glibc 2.34 < 2.39"; return 1 ;;
        alpine|void)
            DISTRO_REASON="alpine/void use musl libc"; return 1 ;;
        opensuse-leap|sles)
            ver_ge "$DISTRO_VERSION" "16.0" \
                || { DISTRO_REASON="${DISTRO_ID} ${DISTRO_VERSION} ships glibc < 2.39"; return 1; } ;;
        opensuse-tumbleweed)
            return 0 ;;
        arch|archarm|manjaro|endeavouros|cachyos|artix|garuda)
            return 0 ;;
        *)
            if [ -z "$DISTRO_FAMILY" ]; then
                DISTRO_REASON="unrecognized distribution '${DISTRO_PRETTY}'"
                return 1
            fi
            return 0
    esac
    return 0
}

unsupported_message() {
    echo ""
    echo "  This distro ${DISTRO_PRETTY} is not supported by mire-lang."
    [ -n "$DISTRO_REASON" ] && echo ""
    [ -n "$DISTRO_REASON" ] && echo "  Reason: ${DISTRO_REASON}"
    echo ""
    echo "  mire releases provide prebuilt binaries that require GNU glibc >= 2.39"
    echo "  plus LLVM/Clang 18-22 (any distribution-native version works)."
    echo ""
    echo "  Supported examples:"
    echo "    • Ubuntu 24.04+, Debian 13+, Fedora 41+"
    echo "    • Oracle Linux 10 / RHEL 10"
    echo "    • Arch Linux (rolling)"
    echo ""
    echo "  What you can do:"
    echo "    • Run in the provided container (recommended for old distros):"
    docker_run_hint
    [ "$CHECK_ONLY" = "1" ] || [ "$YES" = "1" ] || docker_offer
    echo "    • Installation docs:     https://github.com/mire-lang/Avenys-rust#readme"
    echo "    • Official docs:         https://mire-lang.github.io"
    echo "    • Build from source:     https://github.com/mire-lang/Avenys-rust"
    echo "    • Reported platform/help: https://github.com/mire-lang/Avenys-rust/issues"
    echo ""
    echo "  (Override this safety check with MIRE_SKIP_DISTRO_CHECK=1 at your own risk.)"
}

docker_run_hint() {
    echo "          docker pull ${DOCKER_IMAGE}"
    echo "          docker run --rm -it ${DOCKER_IMAGE}"
    echo ""
    echo "        Inside the container the toolchain is already installed:"
    echo "          owl --version && mire --version"
    echo ""
    echo "        To deploy/test a project, mount it read-write:"
    echo "          docker run --rm -it -v \$PWD:/workspace ${DOCKER_IMAGE} \\"
    echo "            bash -c 'cd /workspace && owl run'"
}

# Offer to run the install inside the container instead of aborting.
docker_offer() {
    if command -v docker >/dev/null 2>&1 || [ "$RUN_DOCKER" = "1" ]; then
        echo ""
        if confirm_prompt "  continue inside the ${DOCKER_IMAGE} container?"; then
            run_docker_toolchain
            exit 0
        fi
        echo "  continuing without the container (will abort)."
    fi
}

# Run the toolchain container. The image ships a modern glibc + the full
# mire toolchain, so distros below the 2.39 floor get a working environment.
run_docker_toolchain() {
    echo ""
    echo "  ── Docker toolchain ─────────────────────────────────────────"
    echo ""
    if ! command -v docker >/dev/null 2>&1; then
        echo "  error: docker not found on this host."
        echo "  install the container engine, then run:"
        docker_run_hint
        exit 1
    fi
    echo "  image: ${DOCKER_IMAGE}"
    if ! confirm_prompt "  pull and run it?"; then
        echo "  aborted."
        exit 0
    fi
    as_root docker pull "${DOCKER_IMAGE}" >/dev/null 2>&1 || as_root docker pull "${DOCKER_IMAGE}" || true
    echo "  starting an interactive shell…"
    echo "  (exit with Ctrl-D when done)"
    echo ""
    as_root docker run --rm -it -v "$(pwd):/workspace" "${DOCKER_IMAGE}"
    echo "  container session ended."
}

# Confirmation helper. Non-interactive (no tty) or --yes auto-approves so
# `curl … | sh` never blocks inside scripts, CI, or containers. Avoids
# touching /dev/tty entirely (missing in containers -> shell redirection
# error), so all interactivity is decided from stdin exactly once.
confirm_prompt() {
    if [ "$YES" = "1" ]; then
        return 0
    fi
    if [ ! -t 0 ]; then
        echo "  (non-interactive: assuming yes)"
        return 0
    fi
    printf '%s [Y/n] ' "$1"
    ans=""
    read -r ans || ans=""
    case "$ans" in
        [nN]*) return 1 ;;
    esac
    return 0
}

deps_for_pm() {
    case "$1" in
        apt)
            echo "curl tar git clang llvm lld pkg-config libssl-dev libsodium-dev zlib1g-dev libzstd-dev libarchive-dev"
            ;;
        pacman)
            echo "curl tar git clang llvm lld pkg-config openssl libsodium zlib zstd libarchive"
            ;;
        dnf|yum)
            echo "curl tar git clang llvm lld pkgconf-pkg-config openssl-devel zlib-devel libzstd-devel libarchive-devel"
            ;;
        apk)
            echo "curl tar git clang llvm lld openssl-dev libsodium-dev zlib-dev zstd-dev libarchive-dev"
            ;;
        zypper)
            echo "curl tar git clang llvm lld pkg-config libopenssl-devel libsodium-devel zlib-devel libzstd-devel libarchive-devel"
            ;;
        *) echo "" ;;
    esac
}

install_deps() {
    local pm="$1"
    local pkgs refresh do_refresh
    pkgs="$(deps_for_pm "$pm")"
    case "$pm" in
        apt)          refresh="apt-get update" ;;
        pacman)       refresh="pacman -Sy" ;;
        dnf|yum)      refresh="$pm makecache" ;;
        apk)          refresh="apk update" ;;
        zypper)       refresh="zypper refresh" ;;
        *)            refresh="" ;;
    esac
    echo ""
    echo "  ── Installing build prerequisites ────────────────────────────"
    echo ""
    echo "  The installer will:"
    [ -n "$refresh" ] && echo "    1. refresh package indexes:  ${refresh}"
    echo "    2. install packages:  ${pkgs:-<none detected>}"
    echo ""
    do_refresh=0
    if [ -n "$refresh" ] && confirm_prompt "  refresh package indexes (${refresh})?"; then
        do_refresh=1
    fi
    if ! confirm_prompt "  install the packages listed above?"; then
        echo "  skipping dependency install"
        return 2
    fi

    case "$pm" in
        apt)
            if [ "$do_refresh" = "1" ]; then
                as_root apt-get update -qq \
                    || echo "  note: package index refresh failed; continuing"
            fi
            as_root apt-get install -y -qq $pkgs
            ;;
        pacman)
            as_root pacman -Sy --needed --noconfirm $pkgs
            ;;
        dnf|yum)
            [ "$do_refresh" = "1" ] && as_root "$pm" -y makecache >/dev/null 2>&1 || true
            as_root "$pm" -y install $pkgs
            # libsodium-devel is NOT in every RHEL-family repo. On Oracle
            # Linux / RHEL 10 it ships via EPEL: try the native package, then
            # enable EPEL and retry. Not fatal when unavailable (warning only).
            if [ "$pm" = "dnf" ]; then
                as_root dnf -y install libsodium-devel >/dev/null 2>&1 \
                    || { as_root dnf -y -q install oracle-epel-release-el10 >/dev/null 2>&1 \
                         || as_root dnf -y -q install epel-release >/dev/null 2>&1;
                         as_root dnf -y install libsodium-devel >/dev/null 2>&1; } \
                    || echo "  note: libsodium-devel unavailable (EPEL); full-tier crypto links need it, minimal-tier projects do not."
            fi
            ;;
        apk)
            as_root apk add $pkgs
            ;;
        zypper)
            as_root zypper --non-interactive install -y $pkgs
            ;;
        *)
            echo "  warning: no package manager available; install the tools manually"
            ;;
    esac
}

# Major version of the newest available LLVM tool, or "0" when none.
llvm_major() {
    for t in opt llc clang; do
        if command -v "$t" >/dev/null 2>&1; then
            "$t" --version 2>/dev/null \
                | sed -n 's/.*version \([0-9][0-9]*\)\..*/\1/p' | head -1 \
                && return 0
        fi
    done
    echo "0"
}

# Path to an llvm-config usable by Avenys' inkwell for a from-source build
# (LLVM 22 down to 18 are accepted). Prefers the -NNN suffixed binaries.
llvm_config_for_inkwell() {
    local v
    for v in 22 21 20 19 18; do
        if command -v "llvm-config-${v}" >/dev/null 2>&1; then
            echo "llvm-config-${v}"
            return 0
        fi
    done
    command -v llvm-config 2>/dev/null || echo "llvm-config"
}

prereq_missing() {
    local missing=""
    if ! command -v curl >/dev/null 2>&1 && ! command -v wget >/dev/null 2>&1; then
        missing="$missing curl"
    fi
    if ! command -v tar >/dev/null 2>&1; then
        missing="$missing tar"
    fi
    if [ "$INSTALL_KIOTO" = "1" ] && ! command -v git >/dev/null 2>&1; then
        missing="$missing git"
    fi

    if [ "$INSTALL_COMPILER" = "1" ] || [ "$COMPILER_ONLY" = "1" ]; then
        command -v clang >/dev/null 2>&1 || missing="$missing clang"
        for t in opt llc llvm-ar ld.lld; do
            command -v "$t" >/dev/null 2>&1 || missing="$missing $t"
        done
        llvm_version="$(llvm_major)"
        if [ -z "$llvm_version" ] || [ "$llvm_version" -lt 18 ] 2>/dev/null; then
            missing="$missing llvm>=18"
        fi
        if ! command -v pkg-config >/dev/null 2>&1; then
            missing="$missing pkg-config"
        else
            # libsodium is optional (best-effort, see install_deps): full-tier
            # crypto linking needs it, minimal-tier projects do not.
            for pc in openssl zlib libzstd libarchive; do
                pkg-config --exists "$pc" 2>/dev/null || missing="$missing pc:$pc"
            done
        fi
    fi

    if [ "$KIOTO_ONLY" != "1" ] && command -v ldconfig >/dev/null 2>&1 \
        && ! ldconfig -p 2>/dev/null | grep -q 'libsodium.so'; then
        missing="$missing libsodium"
    fi

    echo "$missing"
}

check_prerequisites() {
    local missing core_missing
    missing="$(prereq_missing)"
    if [ -z "$missing" ]; then
        return 0
    fi

    echo "  missing:${missing}"
    if [ "$CHECK_ONLY" = "1" ]; then
        return 1
    fi

    local pm
    pm="$(detect_pkg_manager)"
    if [ "$pm" = "none" ]; then
        echo "  install these and re-run."
        return 1
    fi

    install_deps "$pm" || true
    missing="$(prereq_missing)"
    if [ -n "$missing" ]; then
        # libsodium alone is a warning, not a blocker: full-tier crypto linking
        # needs it, minimal-tier projects (owl.toml default) do not.
        core_missing="${missing/ libsodium/}"
        if [ -z "$core_missing" ]; then
            echo "  note: libsodium not available on this distro"
            echo "        full-tier crypto linking needs libsodium (see EPEL on RHEL-family)"
            echo "        minimal-tier projects work without it"
            return 0
        fi
        echo "  still missing after install:${core_missing}"
        return 1
    fi
}

get_latest_tag() {
    local repo="$1"
    local tag=""
    if command -v curl >/dev/null 2>&1; then
        tag="$(curl -fsSL "${API_URL}/repos/${repo}/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
        if [ -z "$tag" ]; then
            tag="$(curl -fsSL "${BASE_URL}/${repo}/releases/latest" 2>/dev/null \
                | grep -oE '/releases/tag/v[^"]+' | head -1 | sed 's|/releases/tag/||')"
        fi
    elif command -v wget >/dev/null 2>&1; then
        tag="$(wget -qO- "${API_URL}/repos/${repo}/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
        if [ -z "$tag" ]; then
            tag="$(wget -qO- "${BASE_URL}/${repo}/releases/latest" 2>/dev/null \
                | grep -oE '/releases/tag/v[^"]+' | head -1 | sed 's|/releases/tag/||')"
        fi
    fi
    echo "$tag"
}

detect_distro
detect_glibc
DISTRO_FAMILY="$(distro_family)"

HOST_ARCH="$(detect_arch)"
TARGET="${ARCH_TARGET:-$HOST_ARCH}"
if [ -z "$ARCH_TARGET" ] || [ "$ARCH_TARGET" = "$HOST_ARCH" ]; then
    ARCH_SRC="detected"
else
    ARCH_SRC="requested"
fi

echo ""
echo "  system: ${DISTRO_PRETTY:-unknown}"
if [ "$LIBC_MUSL" = "1" ]; then
    echo "  libc:   musl"
else
    echo "  libc:   glibc ${GLIBC_VERSION:-unknown}"
fi
echo "  arch:   ${TARGET} (${ARCH_SRC})"
echo "  pkg:    $(detect_pkg_manager)"

if [ "$RUN_DOCKER" = "1" ]; then
    run_docker_toolchain
    exit 0
fi

if ! arch_supported; then
    echo ""
    echo "  architecture ${TARGET} has no prebuilt release archives."
    echo ""
    echo "  Supported release architectures:"
    echo "    • x86_64-unknown-linux-gnu   (amd64)"
    echo "    • aarch64-unknown-linux-gnu  (arm64)"
    echo "    • riscv64-unknown-linux-gnu  (riscv64)"
    echo ""
    echo "  Use --arch <target> to pick a supported one, or run the toolchain"
    echo "  in the provided container (--docker)."
    exit 1
fi

select_arch

if ! check_distro_support; then
    unsupported_message
    if [ "$CHECK_ONLY" = "1" ]; then
        exit 1
    fi
    echo ""
    echo "  aborting before any modification."
    echo ""
    exit 1
fi

# --check: report and stop. Never touches the network or the system.
if [ "$CHECK_ONLY" = "1" ]; then
    echo ""
    echo "  audit report"
    echo "  ────────────"
    echo "  system : ${DISTRO_PRETTY:-unknown}"
    if [ "$LIBC_MUSL" = "1" ]; then
        echo "  libc   : musl (not supported)"
    else
        echo "  libc   : glibc ${GLIBC_VERSION:-unknown}"
    fi
    echo "  arch   : ${TARGET} (${ARCH_SRC})"
    echo "  pkg mgr: $(detect_pkg_manager)"
    echo "  llvm   : $(llvm_major) (need >= 18 for compiles)"
    if [ "$BUILD_FROM_SOURCE" = "1" ]; then
        if command -v rustc >/dev/null 2>&1; then
            echo "  rust   : $(rustc --version 2>/dev/null | head -1) (min ${MIN_RUST_VERSION})"
        else
            echo "  rust   : NOT INSTALLED (required by --build-from-source)"
        fi
        echo "  source : ${SOURCE_URL} (ref ${SOURCE_REF})"
    fi
    echo ""
    if check_prerequisites; then
        echo "  prerequisites check complete — all present."
        exit 0
    fi
    echo "  prerequisites check FAILED — see 'missing' below."
    exit 1
fi

check_prerequisites

# Resolve the compiler/owl release tag only when a release download is used
# (owl always downloads; a prebuilt compiler only when not building from source).
if [ "$KIOTO_ONLY" != "1" ] \
    && { [ "$INSTALL_OWL" = "1" ] || [ "$BUILD_FROM_SOURCE" != "1" ]; }; then
    if [ -z "$TAG_COMPILER" ]; then
        TAG_COMPILER="$(get_latest_tag "$REPO_COMPILER")"
        if [ -z "$TAG_COMPILER" ]; then
            echo "  warning: could not determine latest release tag"
            echo "  set MIRE_BASE_URL/MIRE_API_URL or use --tag-compiler"
            exit 1
        fi
    fi
fi

install_file() {
    local src="$1" dst="$2"
    if needs_sudo; then
        as_root mkdir -p "$(dirname "$dst")"
        as_root cp "$src" "$dst"
        as_root chmod +x "$dst"
    else
        mkdir -p "$(dirname "$dst")"
        cp "$src" "$dst"
        chmod +x "$dst"
    fi
}

install_dir() {
    local src="$1"
    local name
    name="$(basename "$src")"
    if needs_sudo; then
        as_root mkdir -p "${LIB_DIR}"
        as_root rm -rf "${LIB_DIR}/${name}"
        as_root cp -r "$src" "${LIB_DIR}/"
    else
        mkdir -p "${LIB_DIR}"
        rm -rf "${LIB_DIR}/${name}"
        cp -r "$src" "${LIB_DIR}/"
    fi
}

detect_shell_profile() {
    local sh
    sh="$(basename "${SHELL:-/bin/sh}")"
    case "$sh" in
        zsh)  echo "${ZDOTDIR:-$HOME}/.zshrc" ;;
        fish) echo "$HOME/.config/fish/config.fish" ;;
        bash)
            if [ -f "$HOME/.bash_profile" ]; then
                echo "$HOME/.bash_profile"
            else
                echo "$HOME/.bashrc"
            fi
            ;;
        *)    echo "$HOME/.profile" ;;
    esac
}

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

OWL_BIN=""
COMPILER_BIN=""

# ── Kioto only ───────────────────────────────────────────────────────
if [ "$KIOTO_ONLY" = "1" ]; then
    echo ""
    echo "  ── Installing kioto stdlib ───────────────────────────────────"
    echo ""

    if ! confirm_prompt "  continue?"; then
        echo "  aborted."
        exit 0
    fi

    mkdir -p "${OWL_HOME}/libs" "${OWL_HOME}/tmp" "${OWL_HOME}/cfg"

    if [ ! -f "$OWL_HOME/config.toml" ]; then
        cat > "$OWL_HOME/config.toml" <<CONFIG
[owl]
version = "1.0.0"

[libs]
path = "$OWL_HOME/libs"

[download]
timeout = 30
retry = 3
CONFIG
        echo "  created ${OWL_HOME}/config.toml"
    fi

    TAG_KIOTO="${TAG_KIOTO:-$(get_latest_tag "$REPO_KIOTO")}"
    echo "  downloading kioto ${TAG_KIOTO:-latest}..."

    set -e
    cd "$TMPDIR"
    rm -rf kioto-clone
    if command -v git >/dev/null 2>&1; then
        git clone --depth 1 --branch "${TAG_KIOTO:-main}" "${KIOTO_URL}" kioto-clone 2>/dev/null \
            || git clone --depth 1 "${KIOTO_URL}" kioto-clone
    else
        echo "  error: git is required for kioto-only install"
        exit 1
    fi

    rm -rf "${OWL_HOME}/libs/kioto"
    cp -r kioto-clone "${OWL_HOME}/libs/kioto"

    echo "  installed kioto to ${OWL_HOME}/libs/kioto/"
    echo ""
    echo "  ─────────────────────────────────────────────────────────────────"
    echo "  kioto install complete"
    echo ""
    echo "  Owl will resolve kioto from ${OWL_HOME}/libs/kioto/"
    echo ""
    exit 0
fi

# ── Install owl + kioto ───────────────────────────────────────────────
if [ "$INSTALL_OWL" = "1" ]; then
    echo ""
    echo "  ── Installing owl ──────────────────────────────────────────────"
    echo ""

    if ! confirm_prompt "  continue?"; then
        echo "  aborted."
        exit 0
    fi

    OWL_TARBALL="${OWL_TARBALL:-owl-${TARGET}.tar.gz}"
    OWL_URLS="${BASE_URL}/${REPO_COMPILER}/releases/download/${TAG_COMPILER}/${OWL_TARBALL}"
    if [ -z "$ARCH_TARGET" ] && [ "$TARGET" = "x86_64-unknown-linux-gnu" ] \
        && [ "$OWL_TARBALL" = "owl-${TARGET}.tar.gz" ]; then
        OWL_URLS="${OWL_URLS} ${BASE_URL}/${REPO_COMPILER}/releases/download/${TAG_COMPILER}/${OWL_TARBALL_LEGACY}"
    fi

    echo "  downloading owl ${TAG_COMPILER} (${OWL_TARBALL})..."
    OWL_DL="$(fetch_first "$TMPDIR/$OWL_TARBALL" $OWL_URLS)"

    if [ -f "$TMPDIR/$OWL_TARBALL" ] && [ -s "$TMPDIR/$OWL_TARBALL" ]; then
        echo "  extracting..."
        tar xzf "$TMPDIR/$OWL_TARBALL" -C "$TMPDIR"

        if [ -f "$TMPDIR/owl/owl" ]; then
            install_file "$TMPDIR/owl/owl" "${BIN_DIR}/owl"
            OWL_BIN="${BIN_DIR}/owl"
        fi

        if [ -d "$TMPDIR/owl/kioto" ] && [ "$INSTALL_KIOTO" = "1" ]; then
            mkdir -p "${OWL_HOME}/libs" "${OWL_HOME}/tmp" "${OWL_HOME}/cfg"
            rm -rf "${OWL_HOME}/libs/kioto"
            cp -r "$TMPDIR/owl/kioto" "${OWL_HOME}/libs/kioto"
        fi

        # Bundled shared libraries (e.g. libsodium.so.23) that the owl binary
        # resolves via its baked-in RUNPATH $ORIGIN/../lib/mire. Installed
        # alongside the compiled runtime so owl runs without system libsodium.
        if [ -d "$TMPDIR/owl/lib" ]; then
            mkdir -p "${LIB_DIR}"
            for lib in "$TMPDIR"/owl/lib/*.so*; do
                if [ -f "$lib" ]; then
                    cp -f "$lib" "${LIB_DIR}/$(basename "$lib")"
                    echo "  bundled ${LIB_DIR}/$(basename "$lib")"
                fi
            done
        fi

        if [ ! -f "$OWL_HOME/config.toml" ]; then
            mkdir -p "$OWL_HOME"
            cat > "$OWL_HOME/config.toml" <<CONFIG
[owl]
version = "1.0.0"

[libs]
path = "$OWL_HOME/libs"

[download]
timeout = 30
retry = 3
CONFIG
        fi
    else
        echo "  error: could not download owl release"
        echo "  tag: ${TAG_COMPILER}"
        echo "  url: ${OWL_URLS}"
        if [ "$INSTALL_COMPILER" != "1" ] && [ "$COMPILER_ONLY" != "1" ]; then
            exit 1
        fi
    fi
fi

# ── mire stdlib (best-effort) ────────────────────────────────────────
# The ``mire`` package (mire-lang/mire-lib) is required to build projects
# that `load mire` / `load kioto`. Download the source archive from the
# repository (mirrorable via MIRE_MIRE_URL / MIRE_MIRE_REF). Not fatal:
# standalone `mire build` of plain programs (no `load`) works without it.
if [ "$INSTALL_KIOTO" = "1" ] && [ "$KIOTO_ONLY" != "1" ] && [ -z "$MIRE_NO_STDLIB" ]; then
    if [ ! -d "${OWL_HOME}/libs/mire" ]; then
        echo ""
        echo "  ── Installing mire stdlib (mire-lang/mire-lib) ───────────────"
        MIRE_MIRE_REF="${MIRE_MIRE_REF:-main}"
        MIRE_STDLIB_URL="${MIRE_MIRE_URL:-${BASE_URL}/${REPO_MIRE}/archive/refs/heads/${MIRE_MIRE_REF}.tar.gz}"
        echo "  downloading mire stdlib @${MIRE_MIRE_REF}..."
        if command -v curl >/dev/null 2>&1; then
            curl -fsSL "$MIRE_STDLIB_URL" -o "$TMPDIR/mire-lib.tar.gz" 2>/dev/null || true
        elif command -v wget >/dev/null 2>&1; then
            wget -q "$MIRE_STDLIB_URL" -O "$TMPDIR/mire-lib.tar.gz" 2>/dev/null || true
        fi
        if [ -f "$TMPDIR/mire-lib.tar.gz" ] && [ -s "$TMPDIR/mire-lib.tar.gz" ]; then
            mkdir -p "$TMPDIR/mirelib-x"
            tar xzf "$TMPDIR/mire-lib.tar.gz" -C "$TMPDIR/mirelib-x" 2>/dev/null
            SRC=""
            for d in "$TMPDIR"/mirelib-x/*/; do
                [ -f "$d/owl.toml" ] && [ -d "$d/core" ] && SRC="$d" && break
            done
            if [ -n "$SRC" ]; then
                mkdir -p "${OWL_HOME}/libs"
                rm -rf "${OWL_HOME}/libs/mire"
                cp -r "$SRC" "${OWL_HOME}/libs/mire"
                echo "  installed mire stdlib to ${OWL_HOME}/libs/mire/"
            else
                echo "  note: mire stdlib archive had an unexpected layout; skipping"
            fi
        else
            echo "  note: could not download mire stdlib (best-effort, continuing)"
        fi
    else
        echo "  using existing mire stdlib at ${OWL_HOME}/libs/mire/"
    fi
fi

# ── Build compiler from source ────────────────────────────────────────
# --build-from-source compiles Avenys locally instead of downloading the
# prebuilt release archive. Only the compiler is built this way; the owl and
# kioto release artifacts are Mire programs / stdlib (not host binaries), so
# they still come from the release as usual.
if [ "$BUILD_FROM_SOURCE" = "1" ]; then
    echo ""
    echo "  ── Building Mire compiler from source ───────────────────────"
    echo "  source: ${SOURCE_URL} (ref ${SOURCE_REF})"
    echo "  rust:   ${MIN_RUST_VERSION}+ required (edition 2024)"
    echo ""

    if ! confirm_prompt "  continue?"; then
        echo "  aborted."
        exit 0
    fi

    # 1. Rust / Cargo presence ─────────────────────────────────────────
    if ! command -v rustc >/dev/null 2>&1 || ! command -v cargo >/dev/null 2>&1; then
        echo "  rustc/cargo not found."
        echo "  The official guide is https://rustup.rs (rustup installs rustc"
        echo "  and cargo and defaults to the latest stable release)."
        if ! confirm_prompt "  install Rust via rustup (official guide) now?"; then
            echo "  aborted."
            exit 0
        fi
        if command -v curl >/dev/null 2>&1; then
            curl -fsSL "$RUSTUP_URL" -o "$TMPDIR/rustup-init.sh" 2>/dev/null || true
        elif command -v wget >/dev/null 2>&1; then
            wget -q "$RUSTUP_URL" -O "$TMPDIR/rustup-init.sh" 2>/dev/null || true
        fi
        if [ -s "$TMPDIR/rustup-init.sh" ]; then
            sh "$TMPDIR/rustup-init.sh" -y --profile minimal >/dev/null 2>&1 || true
        fi
        if command -v "$HOME/.cargo/bin/rustc" >/dev/null 2>&1; then
            export PATH="$HOME/.cargo/bin:$PATH"
        fi
        if ! command -v rustc >/dev/null 2>&1; then
            echo "  error: rustc is still unavailable after the rustup attempt."
            echo "  run the official installer yourself: https://rustup.rs"
            echo "  then re-run this script."
            exit 1
        fi
    fi

    # 2. Version floor ──────────────────────────────────────────────────
    RUST_VER="$(rustc --version 2>/dev/null | sed -E 's/rustc ([0-9]+\.[0-9]+).*/\1/')"
    if [ -n "$RUST_VER" ] && ! ver_ge "$RUST_VER" "$MIN_RUST_VERSION"; then
        echo ""
        echo "  rustc ${RUST_VER} is older than the recommended ${MIN_RUST_VERSION}."
        echo "  Official guide: https://rustup.rs (rustup update stable)."
        printf '  update Rust/cargo now (via rustup) or cancel? [S] update / [N] cancel: '
        choice=""
        read -r choice || choice="N"
        case "$choice" in
            [sS]*)
                if command -v rustup >/dev/null 2>&1; then
                    echo "  running: rustup update stable (see https://rustup.rs)"
                    rustup update stable 2>/dev/null || true
                    if command -v "$HOME/.cargo/bin/rustc" >/dev/null 2>&1; then
                        export PATH="$HOME/.cargo/bin:$PATH"
                    fi
                else
                    echo "  error: rustup not present; install it per https://rustup.rs"
                    echo "  and re-run."
                    exit 1
                fi
                ;;
            *)
                echo "  cancelled."
                if confirm_prompt "  delete everything created so far?"; then
                    rm -rf "$TMPDIR"
                fi
                echo "  nothing was installed. Update Rust (https://rustup.rs) and re-run."
                exit 1
                ;;
        esac
    fi
    echo "  rust:   $(rustc --version 2>/dev/null | head -1)"

    # 3. Native toolchain (must exist for a build; --check/install_deps above
    #    already tried to arrange it, but a bare source build may not have it).
    if ! command -v clang >/dev/null 2>&1 || ! command -v llc >/dev/null 2>&1 \
        || ! command -v ld.lld >/dev/null 2>&1; then
        echo "  error: clang/llc/ld.lld missing — a build needs the LLVM tools."
        echo "  re-run with --compiler (no --build-from-source) so the script can"
        echo "  install them via your package manager, or install them manually."
        exit 1
    fi

    # 4. Fetch source ───────────────────────────────────────────────────
    echo "  fetching source from ${SOURCE_URL}..."
    if ! command -v git >/dev/null 2>&1 && ! command -v curl >/dev/null 2>&1 \
        && ! command -v wget >/dev/null 2>&1; then
        echo "  error: git or curl required to fetch the Avenys source."
        exit 1
    fi
    if [ -d "$TMPDIR/avenys-src" ]; then
        rm -rf "$TMPDIR/avenys-src"
    fi
    if command -v git >/dev/null 2>&1; then
        if ! git clone -q --depth 1 --branch "$SOURCE_REF" "$SOURCE_URL" "$TMPDIR/avenys-src" \
            && ! git clone -q --depth 1 "$SOURCE_URL" "$TMPDIR/avenys-src"; then
            echo "  error: could not clone ${SOURCE_URL}"
            exit 1
        fi
    else
        SRC_TARBALL="${BASE_URL}/${REPO_COMPILER}/archive/refs/heads/${SOURCE_REF}.tar.gz"
        if command -v curl >/dev/null 2>&1; then
            curl -fsSL "$SRC_TARBALL" -o "$TMPDIR/avenys-src.tar.gz" || { echo "  error: source download failed"; exit 1; }
        else
            wget -q "$SRC_TARBALL" -O "$TMPDIR/avenys-src.tar.gz" || { echo "  error: source download failed"; exit 1; }
        fi
        mkdir -p "$TMPDIR/avenys-src-x"
        tar xzf "$TMPDIR/avenys-src.tar.gz" -C "$TMPDIR/avenys-src-x"
        SRCX=""
        for d in "$TMPDIR"/avenys-src-x/*/; do
            [ -f "$d/Cargo.toml" ] && SRCX="$d" && break
        done
        [ -n "$SRCX" ] || { echo "  error: source archive had an unexpected layout"; exit 1; }
        mv "$SRCX" "$TMPDIR/avenys-src"
    fi

    # 5. Compile (inkwell needs LLVM_CONFIG_PATH pointing at a matching llvm-config)
    echo "  compiling (this can take a few minutes)…"
    ( cd "$TMPDIR/avenys-src" \
        && LLVM_CONFIG_PATH="$(llvm_config_for_inkwell)" cargo build --release --locked ) \
        || { echo "  error: cargo build failed."; exit 1; }

    # 6. Install ────────────────────────────────────────────────────────
    if [ -f "$TMPDIR/avenys-src/target/release/mire" ]; then
        install_file "$TMPDIR/avenys-src/target/release/mire" "${BIN_DIR}/mire"
        COMPILER_BIN="${BIN_DIR}/mire"
    else
        echo "  error: compiled binary not found at target/release/mire"
        exit 1
    fi
    if [ -d "$TMPDIR/avenys-src/src/runtime" ]; then
        install_dir "$TMPDIR/avenys-src/src/runtime"
    fi
    if [ -d "$TMPDIR/avenys-src/src/pal" ]; then
        install_dir "$TMPDIR/avenys-src/src/pal"
    fi
    COMPILER_SOURCE_INSTALLED=1
    echo "  compiler built and installed from source."
fi

# ── Install compiler (prebuilt release archive) ───────────────────────
if { [ "$INSTALL_COMPILER" = "1" ] || [ "$COMPILER_ONLY" = "1" ]; } \
    && [ "${COMPILER_SOURCE_INSTALLED:-0}" != "1" ]; then
    echo ""
    echo "  ── Installing Mire compiler ──────────────────────────────────"
    echo ""

    if ! confirm_prompt "  continue?"; then
        echo "  aborted."
        exit 0
    fi

    COMPILER_TARBALL="${COMPILER_TARBALL:-mire-compiler-${TARGET}.tar.gz}"
    COMPILER_URLS="${BASE_URL}/${REPO_COMPILER}/releases/download/${TAG_COMPILER}/${COMPILER_TARBALL}"
    if [ -z "$ARCH_TARGET" ] && [ "$TARGET" = "x86_64-unknown-linux-gnu" ] \
        && [ "$COMPILER_TARBALL" = "mire-compiler-${TARGET}.tar.gz" ]; then
        COMPILER_URLS="${COMPILER_URLS} ${BASE_URL}/${REPO_COMPILER}/releases/download/${TAG_COMPILER}/${COMPILER_TARBALL_LEGACY}"
    fi

    echo "  downloading compiler ${TAG_COMPILER} (${COMPILER_TARBALL})..."
    COMPILER_DL="$(fetch_first "$TMPDIR/$COMPILER_TARBALL" $COMPILER_URLS)"

    if [ -f "$TMPDIR/$COMPILER_TARBALL" ] && [ -s "$TMPDIR/$COMPILER_TARBALL" ]; then
        echo "  extracting..."
        tar xzf "$TMPDIR/$COMPILER_TARBALL" -C "$TMPDIR"

        if [ -f "$TMPDIR/compiler/mire" ]; then
            install_file "$TMPDIR/compiler/mire" "${BIN_DIR}/mire"
            COMPILER_BIN="${BIN_DIR}/mire"
        fi

        if [ -d "$TMPDIR/compiler/runtime" ]; then
            install_dir "$TMPDIR/compiler/runtime"
        fi
        if [ -d "$TMPDIR/compiler/pal" ]; then
            install_dir "$TMPDIR/compiler/pal"
        fi
    else
        echo "  error: could not download compiler release"
        echo "  tag: ${TAG_COMPILER}"
        echo "  url: ${COMPILER_URLS}"
        echo ""
        echo "  Build from source:"
        echo "    git clone https://github.com/${REPO_COMPILER}"
        echo "    cd ${REPO_COMPILER}"
        echo "    cargo build --release"
        echo "    cp target/release/mire ${BIN_DIR}/mire"
        if [ "$COMPILER_ONLY" = "1" ]; then
            exit 1
        fi
    fi
fi

# ── PATH setup ────────────────────────────────────────────────────────
if [ "$NO_PROFILE" != "1" ] && [ "$COMPILER_ONLY" != "1" ]; then
    PROFILE_FILE="$(detect_shell_profile)"

    case ":$PATH:" in
        *":$BIN_DIR:"*)
            echo ""
            echo "  ${BIN_DIR} already in PATH"
            ;;
        *)
            echo ""
            echo "  adding ${BIN_DIR} to PATH"

            if ! confirm_prompt "  modify ${PROFILE_FILE}?"; then
                echo "  skipped."
            else
                BACKUP="${PROFILE_FILE}.owl-backup-$(date +%Y%m%d-%H%M%S)"
                if [ -f "$PROFILE_FILE" ]; then
                    cp "$PROFILE_FILE" "$BACKUP"
                else
                    touch "$PROFILE_FILE"
                fi
                echo "  backup: ${BACKUP}"

                cat >> "$PROFILE_FILE" << PATHLINE

# added by Mire install script
export PATH="${BIN_DIR}:\$PATH"
PATHLINE
                echo "  updated ${PROFILE_FILE}"
                echo "  run: source ${PROFILE_FILE}"
            fi
            ;;
    esac
fi

# ── Done ──────────────────────────────────────────────────────────────
echo ""
echo "  ─────────────────────────────────────────────────────────────────"
echo "  install complete"
echo ""
if [ -n "$OWL_BIN" ] && [ -f "$OWL_BIN" ]; then
    echo "  owl:  ${OWL_BIN}"
    "$OWL_BIN" -V 2>/dev/null || true
fi
if [ -n "$COMPILER_BIN" ] && [ -f "$COMPILER_BIN" ]; then
    echo "  mire: ${COMPILER_BIN}"
    "$COMPILER_BIN" --version 2>/dev/null || true
fi
echo ""
if [ "$COMPILER_ONLY" != "1" ]; then
    echo "  try:"
    if [ -n "$OWL_BIN" ] && [ -f "$OWL_BIN" ]; then
        echo "    owl --help"
        echo "    owl new my-project"
        echo "    owl run"
    fi
    echo ""
    echo "  Need the compiler?"
    echo "    curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --compiler"
fi
echo ""
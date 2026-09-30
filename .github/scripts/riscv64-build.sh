#!/bin/sh
# riscv64-build.sh — build the Mire compiler (or owl) as a native riscv64
# GNU/Linux binary inside a riscv64 container running under QEMU emulation.
# Run it from an amd64/arm64 host after enabling binfmt_misc, e.g.:
#
#   docker run --rm --privileged tonistiigi/binfmt --install all
#   docker run --rm --platform linux/riscv64 \
#       -v "$PWD":/buildsrc riscv64/ubuntu:26.04 \
#       /buildsrc/.github/scripts/riscv64-build.sh /buildsrc compiler
#
# <workdir> must contain, mounted read-write:
#   compiler mode: the Avenys checkout (Cargo.toml at its root)
#   owl mode:      owl-src/ (owl checkout), kioto/ (kioto checkout); the
#                  mire binary must already be installed into the container
#                  (e.g. from the compiler-mode artifact).
#
# WHY ubuntu:26.04: apt.llvm.org ships no riscv64 packages, but Ubuntu 26.04
# (resolute) provides llvm-22 / llvm-22-dev / clang-22 / lld-22 for riscv64 in
# its ports repo. Inkwell's `llvm22-1` feature requires LLVM >= 22, < 23.

set -e

SRC="${1:?usage: riscv64-build.sh <workdir> <compiler|owl>}"
MODE="${2:-compiler}"

[ -d "$SRC" ] || { echo "error: $SRC is not a directory" >&2; exit 1; }
cd "$SRC"

export DEBIAN_FRONTEND=noninteractive

echo "== riscv64 build: installing base toolchain =="
apt-get update -qq
apt-get install -y -qq \
    curl wget ca-certificates build-essential git pkg-config \
    libsodium-dev zlib1g-dev libzstd-dev libarchive-dev \
    llvm-22 llvm-22-dev llvm-22-tools clang-22 lld-22 libpolly-22-dev \
    || { echo "error: apt install failed (llvm-22 requires Ubuntu 26.04 riscv64 ports)" >&2; exit 1; }

# mire invokes clang/llc/opt/ld.lld by their plain names; provide wrappers.
for t in clang clang++ llc opt llvm-config llvm-ar llvm-nm llvm-objcopy llvm-ranlib llvm-strip ld.lld; do
    if [ -x "/usr/bin/${t}-22" ]; then
        ln -sf "${t}-22" "/usr/bin/${t}"
    fi
done

echo "== riscv64 build: installing rust =="
if ! command -v rustc >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
export PATH="$HOME/.cargo/bin:$PATH"
export LLVM_CONFIG_PATH=/usr/bin/llvm-config-22

case "$MODE" in
    compiler)
        echo "== riscv64 build: compiling Avenys (LLVM 22) =="
        command -v clang-22 >/dev/null 2>&1
        cargo build --release --locked
        test -x target/release/mire && echo "mire riscv64 OK"
        ;;
    owl)
        # MIRE_TARBALL (optional): a mire-compiler-*.tar.gz to install into the
        # container before building owl (used by the riscv64 owl job, which runs
        # in a fresh container without the compiler-mode checkout).
        if [ -n "${MIRE_TARBALL:-}" ]; then
            echo "== installing mire from ${MIRE_TARBALL} =="
            mdir="$(mktemp -d)"
            tar xzf "$MIRE_TARBALL" -C "$mdir"
            cp "$mdir/compiler/mire" /usr/local/bin/mire
            chmod +x /usr/local/bin/mire
            mkdir -p /usr/local/lib/mire
            cp -r "$mdir/compiler/runtime" /usr/local/lib/mire/runtime
            cp -r "$mdir/compiler/pal" /usr/local/lib/mire/pal
            rm -rf "$mdir"
        fi
        command -v mire >/dev/null 2>&1 || { echo "error: mire is not on PATH" >&2; exit 1; }
        [ -d owl-src ] || { echo "error: owl-src checkout missing" >&2; exit 1; }
        [ -d kioto ] || { echo "error: kioto checkout missing" >&2; exit 1; }

        echo "== riscv64 build: generating avenys config (owl) =="
        cat > owl-src/native/mire-config.toml <<'EOF'
[project]
name = "owl"
version = "1.1.1"
entry = "code/main.mire"

[build]
profile = "release"
opt-level = 3
artifact = "bin"
runtime = "minimal"
target = "riscv64-unknown-linux-gnu"

[paths]
bin = "/buildsrc/owl-src/bin"
cache = "/tmp/owl-archive-cache"

[cfg]
libs = ["archive"]
sources = ["native/archive.c"]
link-dirs = []
cflags = ["-Wl,--disable-new-dtags", "-Wl,-rpath,$ORIGIN/../lib/mire"]
EOF

        mkdir -p "$HOME/.owl/modules"
        rm -rf "$HOME/.owl/modules/kioto"
        cp -r kioto "$HOME/.owl/modules/kioto"
        # owl-src/owl.toml hardcodes a local kioto path; repoint to the
        # sibling checkout so `load kioto` resolves inside the container.
        sed -i 's|"/home/Evelyn/.owl/libs/kioto"|"../kioto"|' owl-src/owl.toml

        echo "== riscv64 build: compiling owl =="
        ( cd owl-src && mire build code/main.mire --release -O3 --no-analysis-cache --config native/mire-config.toml )
        test -x owl-src/bin/release/main && echo "owl riscv64 OK"

        # Bundle the libsodium runtime lib (owl resolves it via its RUNPATH
        # $ORIGIN/../lib/mire; the installer copies owl/lib/*.so* there).
        mkdir -p owl-src/lib
        find /usr/lib -name 'libsodium.so.23*' -exec cp -a {} owl-src/lib/ \;
        ;;
    *)
        echo "error: unknown mode '$MODE' (expected compiler|owl)" >&2
        exit 1
        ;;
esac
#!/usr/bin/env bash
set -euo pipefail

installer_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/mocks" "$tmp/home"
cat >"$tmp/mocks/sudo" <<'MOCK'
#!/usr/bin/env bash
exec "$@"
MOCK
cat >"$tmp/mocks/apt-get" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$APT_LOG"
MOCK
cat >"$tmp/mocks/wget" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$WGET_LOG"
MOCK
cat >"$tmp/mocks/rustc" <<'MOCK'
#!/usr/bin/env bash
exit 0
MOCK
cat >"$tmp/mocks/git" <<'MOCK'
#!/usr/bin/env bash
exit 0
MOCK
chmod +x "$tmp/mocks/sudo" "$tmp/mocks/apt-get" "$tmp/mocks/wget" "$tmp/mocks/rustc" "$tmp/mocks/git"

HOME="$tmp/home" \
MIRE_PREFIX="$tmp/prefix" \
NO_PROFILE=1 \
APT_LOG="$tmp/apt.log" \
WGET_LOG="$tmp/wget.log" \
PATH="$tmp/mocks:/usr/bin:/bin" \
  bash "$installer_dir/install.sh" --from-source >"$tmp/install.log"

grep -q 'install -y -qq curl wget git build-essential pkg-config' "$tmp/apt.log"
grep -q 'install -y -qq clang-22 llvm-22 lld-22 llvm-22-dev llvm-22-tools' "$tmp/apt.log"
grep -q -- '-qO- https://apt.llvm.org/llvm.sh' "$tmp/wget.log"
if grep -q 'llvm-dev' "$tmp/apt.log"; then
    echo "source installer requested the unpinned llvm-dev package" >&2
    exit 1
fi

echo "source installer LLVM 22 package test passed"

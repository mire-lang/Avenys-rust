#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
uninstaller="$repo_root/install/uninstall.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

make_install() {
    local prefix="$1" home="$2"
    mkdir -p "$prefix/bin" "$prefix/lib/mire" "$home/.owl/libs/mire" "$home/.owl/reg" "$home/.mire"
    touch "$prefix/bin/mire" "$prefix/bin/owl" "$prefix/lib/mire/runtime.a"
    printf '[project]\nname = "mire"\n' >"$home/.owl/libs/mire/owl.toml"
    printf '[registry]\n' >"$home/.owl/config.toml"
    printf 'export PATH="%s/bin:$PATH"\n' "$prefix" >"$home/.bashrc"
}

prefix="$tmp/prefix"
home="$tmp/home"
mkdir -p "$home"
make_install "$prefix" "$home"

HOME="$home" MIRE_PREFIX="$prefix" bash "$uninstaller" --dry-run >"$tmp/dry-run.log"
grep -q 'Dry run complete' "$tmp/dry-run.log"
test -x "$prefix/bin/mire" || test -f "$prefix/bin/mire"
test -f "$home/.owl/libs/mire/owl.toml"

if HOME="$home" MIRE_PREFIX="$prefix" bash "$uninstaller" </dev/null >"$tmp/non-tty.log" 2>&1; then
    echo "interactive uninstall unexpectedly accepted missing terminal input" >&2
    exit 1
fi
grep -q 'Interactive uninstall needs a terminal' "$tmp/non-tty.log"
test -f "$prefix/bin/mire"

HOME="$home" MIRE_PREFIX="$prefix" bash "$uninstaller" --force --keep-libs --keep-config --keep-path >"$tmp/keep.log"
test ! -e "$prefix/bin/mire"
test ! -e "$prefix/bin/owl"
test -f "$home/.owl/libs/mire/owl.toml"
test -f "$home/.owl/config.toml"
grep -q "$prefix/bin" "$home/.bashrc"

echo "uninstaller tests passed"

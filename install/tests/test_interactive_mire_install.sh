#!/usr/bin/env bash
set -euo pipefail

installer_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/mocks" "$tmp/home"
cat >"$tmp/mocks/sudo" <<'MOCK'
#!/bin/sh
exec "$@"
MOCK
cat >"$tmp/mocks/apt-get" <<'MOCK'
#!/bin/sh
exit 0
MOCK
cat >"$tmp/mocks/curl" <<'MOCK'
#!/bin/sh
set -eu
output=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o) output="$2"; shift 2 ;;
        -*) shift ;;
        *) shift ;;
    esac
done
[ -n "$output" ] || exit 2
stage="$(mktemp -d)"
mkdir -p "$stage/package"
printf '[project]\nname = "mire"\n' >"$stage/package/owl.toml"
tar czf "$output" -C "$stage" package
rm -rf "$stage"
MOCK
chmod +x "$tmp/mocks/sudo" "$tmp/mocks/apt-get" "$tmp/mocks/curl"

printf '4\n' | script -qec \
    "HOME='$tmp/home' MIRE_PREFIX='$tmp/prefix' PATH='$tmp/mocks:/usr/bin:/bin' bash '$installer_dir/install.sh' --interactive" \
    /dev/null >"$tmp/interactive.log"
rg -q '4\) Mire standard library' "$tmp/interactive.log"
rg -q 'mire installed from source' "$tmp/interactive.log"
rg -q 'Installation Complete' "$tmp/interactive.log"
test -f "$tmp/home/.owl/libs/mire/owl.toml"

if HOME="$tmp/home" MIRE_PREFIX="$tmp/non-tty-prefix" PATH="/usr/bin:/bin" \
    bash "$installer_dir/install.sh" --interactive </dev/null >"$tmp/non-tty.log" 2>&1; then
    echo "interactive mode unexpectedly accepted stdin without a terminal" >&2
    exit 1
fi
rg -q 'Interactive mode needs a terminal' "$tmp/non-tty.log"

echo "interactive Mire installer tests passed"

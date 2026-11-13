#!/usr/bin/env bash
set -euo pipefail

installer="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)/install.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/mocks"

for package_manager in apt-get pacman dnf zypper; do
    cat >"$tmp/mocks/$package_manager" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$0 $*" >>"$INSTALL_LOG"
MOCK
    chmod +x "$tmp/mocks/$package_manager"
done
cat >"$tmp/mocks/pkg-config" <<'MOCK'
#!/usr/bin/env bash
exit 0
MOCK
chmod +x "$tmp/mocks/pkg-config"

install_deps_source="$(sed -n '/^install_deps() {/,/^}/p' "$installer")"
test -n "$install_deps_source"

for selected_manager in apt-get pacman dnf zypper; do
    export INSTALL_LOG="$tmp/$selected_manager.log"
    : >"$INSTALL_LOG"
    SELECTED_MANAGER="$selected_manager" INSTALL_DEPS_SOURCE="$install_deps_source" \
        PATH="$tmp/mocks:/usr/bin:/bin" bash -c '
        SUDO=""
        start_spinner() { :; }
        stop_spinner() { :; }
        command() {
            if [[ "$1" == "-v" && "$#" -eq 2 ]]; then
                case "$2" in
                    apt-get|pacman|dnf|zypper) [[ "$SELECTED_MANAGER" == "$2" ]]; return ;;
                    tcc) return 0 ;;
                esac
            fi
            builtin command "$@"
        }
        eval "$INSTALL_DEPS_SOURCE"
        install_deps runtime
    '

    if ! grep -qE '(^|[[:space:]])git([[:space:]]|$)' "$INSTALL_LOG"; then
        printf 'runtime dependencies for %s did not include git\n' "$selected_manager" >&2
        cat "$INSTALL_LOG" >&2
        exit 1
    fi
    case "$selected_manager" in
        apt-get) headers=libc6-dev ;;
        pacman) headers=glibc ;;
        dnf|zypper) headers=glibc-devel ;;
    esac
    if ! grep -qE "(^|[[:space:]])${headers}([[:space:]]|$)" "$INSTALL_LOG"; then
        printf 'runtime dependencies for %s did not include C headers (%s)\n' "$selected_manager" "$headers" >&2
        cat "$INSTALL_LOG" >&2
        exit 1
    fi
done

echo "runtime installer includes git and C headers for apt, pacman, dnf, and zypper"

# Avenys v4.4.44

**A compiled, ownership-aware systems language with an LLVM backend.**

Avenys is a statically typed programming language designed for clarity and control.
It gives you structs, enums, generics, closures, pattern matching, and a borrow
checker that tracks ownership at compile time - no garbage collector, no runtime
overhead beyond what you ask for.

The compiler, Avenys (mire), translates Mire source through a multi-stage pipeline into
native binaries via LLVM. It ships with Owl (package manager/build tool) and Kioto, 
a standard library covering strings, collections, math, filesystem, processes, and more.

## Installation

### 1) Quick install with mireup (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --mireup
```

### 2) Interactive installation (choose what to install)

```bash
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --interactive
```

When prompted, enter numbers like `1 2 3` or `1,2,3`:
```
1) Avenys (mire compiler)
2) owl (package manager)
3) kioto (library)
4) sdl (library)
5) sqlite (library)
6) wasm (library)
```

### 3) Install from source (full build)

```bash
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | bash -s -- --from-source
```

The installer auto-detects permissions - if run with `sudo` it installs to `/usr/local/bin/`, otherwise to `~/.local/bin/`. 
It always uses `owl install` for libraries and configures the MOR registry automatically.

**Supported architectures:** x86_64, aarch64 (riscv64 is compatible but must be built by hand).

## Uninstallation

To completely remove the Mire toolchain (compiler, owl, libraries, and configuration):

```bash
# Interactive uninstall (prompts for confirmation)
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash

# Force uninstall everything without prompts
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --force

# Dry run to see what would be removed
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --dry-run

# Keep libraries (~/.owl/libs, ~/.mire) but remove binaries and config
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --keep-libs

# Keep configuration (~/.owl/config.toml, registry) but remove binaries and libs
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/uninstall.sh | bash -s -- --keep-config
```

The uninstaller removes:
- Binaries: `mire` and `owl` from `$PREFIX/bin/`
- Libraries: `$PREFIX/lib/mire/`, `~/.owl/libs/`, `~/.mire/`
- Configuration: `~/.owl/config.toml`, `~/.owl/reg/`, `~/.owl/cache/`
- PATH entries from shell profiles (`.bashrc`, `.zshrc`, `.profile`)

## Quick Start

```bash
# Create a new project (uses src/ directory)
owl new myproject
cd myproject
owl run
```

```mire
pub fn main: () {
    use dasu("Hello from Mire!")
}
```

## Important: Always Use owl CLI

**Always use `owl` CLI (package manager). Never use `mire` CLI directly except for compiler development.**

| Task | Use this |
|------|----------|
| Create project | `owl new myproject` |
| Build | `owl build` |
| Run | `owl run` |
| Test | `owl test` |
| Check | `owl check` |
| Dependencies | `owl load <name>` or `owl install <name>` |

### `owl run` vs `mire run` — Equivalence

Owl wraps the compiler and injects the correct library search paths, config,
and security policy so your project "just works." The table below shows the
exact `mire` command that `owl run` expands to, so you can run the compiler
manually when you need to (debugging a build, CI without Owl, etc.).

Assume a project at `~/myproject` with:
- `owl.toml` in `~/myproject/`
- source in `~/myproject/src/`
- tests in `~/myproject/tests/`
- libraries installed under `~/.owl/libs/` (the default)

```bash
# What `owl run` does internally (normalized compiler config at bin/.cache/mire-config.toml)
mire run --config ~/myproject/bin/.cache/mire-config.toml

# Full manual equivalent (no owl.toml required)
mire run src/main.mire \
  --lib-dir ~/.owl/libs \
  --lib-dir ~/myproject/bin/debug/deps \
  --config <(cat <<'EOF'
[project]
name = "myproject"
version = "0.1.0"
entry = "src/main.mire"

[build]
compiler = "mire"
profile = "debug"
opt-level = 0
artifact = "bin"
runtime = "minimal"
target = "x86_64-unknown-linux-gnu"
panic = "abort"
incremental = true
debug-info = true

[paths]
source = "src"
test = "tests"
bin = "bin/debug"
cache = "bin/.cache"
generated = "bin/.cache/generated"

[cfg]
lock = 1
"update@lock" = "Build"
EOF
)

# Key flags you usually need when running mire directly:
#   --lib-dir ~/.owl/libs        # owl's library store (kioto, mire, sdl, etc.)
#   --lib-dir <project>/bin/debug/deps  # lockfile-resolved deps (owl writes this)
#   --config <path>              # Owl-generated TOML (see `owl build --print-config`)
#   --runtime minimal            # Required for tests and stdlib
#   --artifact bin               # Default; use 'shared' for .so export
```

**Passing arguments to your program:**
```bash
owl run -- --my-arg value    # owl forwards everything after --
mire run ... -- --my-arg value
```

**Test runner:**
```bash
owl test                    # auto-generates config, runs tests/ dir
mire test tests/ --config bin/.cache/mire-test-config.toml --lib-dir ~/.owl/libs --lib-dir bin/debug/deps --runtime minimal
```

> **Tip:** `owl build --print-config` prints the normalized config Owl would pass to the compiler. Pipe it to a file and use `--config` with `mire`.

## Library status()

Every official Mire library exposes `pub fn status: () :str`:

```mire
load mire
load kioto
use dasu(mire::status())
use dasu(kioto::status())
```

## MOR Registry

The official registry is at `mire-lang/libs`. Multi-version support:
- `kioto@2.5.0`, `kioto@3.1.0` (default: 3.1.0)
- `mire@1.2.4`, `mire@2.1.0-beta` (default: 2.1.0-beta)
- `owl@1.6.5` (default: 1.6.5)
- `sdl@1.0.1`, `sqlite@0.1.0`, `wasm@0.2.0`, `ilogical@0.1.0`

## Version History

See [CHANGELOG.md](CHANGELOG.md) for the full release notes.

### v4.4.44 - 2026-11-12
**Runtime safety and installation**
- **String addition preserves live operands.** Code generation leaves ownership decisions to MIR drops instead of treating every LLVM `ptr` temporary as owned. This prevents double frees in complex Owl builds.
- **ARM installer defaults are verified.** Mire and Owl versions match their embedded release versions.

### v4.4.43 - 2026-11-12
**Runtime reliability and release validation**
- **Managed string reuse respects physical capacity.** Reused blocks are selected by actual capacity, preventing writes beyond a smaller block after allocator churn.
- **String vectors own stable copies.** Pushing a managed temporary no longer relies on the source expression remaining alive.
- **Installer and release checks validate embedded versions.** Mismatched archives fail clearly instead of claiming the requested release was installed.

### v4.4.42 - 2026-11-11
**Installer & CI**
- **Install/uninstall scripts rewritten.** Single-line progress output, fresh-vs-update detection, per-component flags (`--compiler`, `--owl`, `--kioto`, `--sdl`, `--sqlite`, `--wasm`), environment overrides, `aarch64` asset naming, `patchelf` fallback and MOR registry configuration. The uninstaller runs when piped to either `bash` or `sh`.
- **RHEL 10 support.** The dependency step skips `tcc` and `libsodium` when `dnf` cannot provide them and builds both from source instead of failing.
- **Release workflow builds the compiler only.** The `owl` job moved to the owl repository, which downloads the precompiled `mire` from the matching release.
- **Version alignment.** `mire --version` reports `4.4.42`, and the bundled `owl` reports `1.6.2`.

### v4.4.15 - 2026-11-06
**Compiler & Runtime Performance**
- **64-bit GEP indices.** Array/vector addresses no longer truncate the index to `i32` and re-extend it; `sort_introsort` 102 → 59 ms and `binary_search` 48 → 22 ms, and indices past 2^31 no longer wrap.
- **Hot vector operations moved into Mire.** `len`, `get`, `set`, `pop`, `with_capacity` and `binary::search` read the header directly and inline; `alloc_pressure` 150 → 98 ms, `binary_search` 48 → 22 ms, `vec_pop_drain` 55 → 49 ms. `push` stays a thin extern because codegen already expands it to an inline fast path.
- **Deterministic, cache-line-aligned layout.** Functions are emitted `align 32`, extern wrappers are sorted, and `llc` aligns non-fall-through blocks to 64 bytes. Identical source now produces identical code placement, removing 20 % swings from `vec_growth`.

### v4.4.14 - 2026-11-05
**Compiler & Runtime Performance**
- **Indexing auto-derefs collection references.** `v[i]` works for `&vec[T]`/`&mut vec[T]`, reads and writes.
- **Map indexing handles string keys and untyped `{}`.** `m[k] = v` stores the map pointer returned by the runtime and records the inferred key/value kinds, so kind-fixing insertions are kept and later reads use the same path.
- **Incremental cache key includes the compiler binary**, so rebuilding the compiler invalidates artifacts from the previous build.
- **Vector bounds checks are inline.** Reads and writes load the header length directly instead of calling `rt_list_len`/`rt_check_bounds_i64` per element.
- **`vec::sort` is Mire code compiled by LLVM.** The hot loop walks the buffer through a raw pointer; `sort_introsort` fell from 292 ms to 102 ms.
- **`rt_i64_to_string` writes digits directly** (no `snprintf`); `numeric_format` fell from 487 ms to 274 ms, now slightly faster than the C reference.
- **`str::index::from`** enables allocation-free resumable search; `string_scan` fell from 3677 ms to 17 ms.
- **`ptr`/string comparisons compare contents**, fixing the `Result::Generics` tests.
- **`mire::status()`** added to the library root.

**Installation & Tooling Fixes**
- **Fixed libsodium dependency issue**: Installation script now properly handles bundled `libsodium.so.23` by setting RPATH via `patchelf` or creating a wrapper script with `LD_LIBRARY_PATH`.
- **Added tcc auto-installation**: The installer now attempts to install `tcc` (required for C object compilation) via package manager on apt/pacman/dnf/zypper systems.
- **Created complete uninstaller** (`install/uninstall.sh`): Removes binaries, libraries, configuration, and PATH entries with options for `--force`, `--dry-run`, `--keep-libs`, `--keep-config`, `--keep-path`.

**Owl Project Template Changes**
- **Project directory renamed**: `code/` → `src/` (matches standard conventions)
- **Output directory hardcoded**: Binary output always goes to `bin/` (removed `[paths].output` from `owl.toml`)
- **New entry point config**: Added support for `[project].input` as alternative to `[project].entry` (input takes precedence)
- **CLI file override preserved**: `owl run src/main.mire` still works and overrides config

### v4.4.13 - 2026-10-25
**Library status() function**
- **status() added to mire, kioto, sdl, sqlite, wasm libraries.** Each library now exposes `pub fn status: () :str` returning a formatted report.
- **owl export / owl -e now verifies status().** The export command checks whether the library being exported defines a `status()` function.

**MOR registry (mire-lang/libs)**
- **kioto: 2.5.0 (stable) + 3.1.0 (latest, default)**
- **mire: 1.2.4 (stable) + 2.0.0-beta (latest, default)**
- **sdl, sqlite, wasm, avenys, owl: latest version only**

**CI/CD and installation**
- Install script always gets latest versions via `get_latest_tag`
- Install script uses `owl install` for libraries with MOR registry
- Install script configures MOR registry automatically

### v4.4.12 - 2026-10-24
**CI/CD fixes**
- Fixed missing archive.c from owl build config
- Install pal runtime directory in build-owl
- Add compiler dependencies for tcc-based C object compilation
- Fix tcc error parsing for crypto library auto-detection
- Install script default repositories changed to `mire-lang/` org

## License

GNU General Public License v3.0

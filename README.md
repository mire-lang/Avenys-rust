# Avenys v4.4.13

**A compiled, ownership-aware systems language with an LLVM backend.**

Avenys is a statically typed programming language designed for clarity and control.
It gives you structs, enums, generics, closures, pattern matching, and a borrow
checker that tracks ownership at compile time - no garbage collector, no runtime
overhead beyond what you ask for.

The compiler, Avenys, translates Mire source through a multi-stage pipeline into
native binaries via LLVM. It ships with Kioto, a standard library covering
strings, collections, math, filesystem, processes, and more.

## Recent Changes (v4.4.9)

- **Vector hot paths.** Append, indexed read, pop, and set now read vector
  metadata directly rather than calling `rt_list_len` internally. Bounds checks,
  null behavior, and growth fallback are unchanged.
- **Benchmark suite expanded to 21 kernels.** Six new kernels were added:
  `sort_introsort` (insertion sort), `matrix_rowmajor` (row-major fill /
  column-major sum stride), `binary_search` (probes on sorted data),
  `string_scan` (repeated `str::index`/`str::substr`), `vec_partition`
  (split into even/odd vectors), and `numeric_format`
  (`str::from::i64`/`str::to::i64` round-trip). All 21 kernels verify
  byte-identical output across Mire, Rust, and C. See `bench/README.md`.
- **Fixed NUL truncation in managed strings.** `rt_managed_is_managed` used
  `sizeof(MireManagedString)` (alignment-rounded to 32) as the arena offset
  lower bound, but string data begins at `offsetof(..., data)` (28). The first
  arena allocation was never recognized as managed, so
  `rt_managed_ensure_managed` re-copied it through `rt_managed_from_cstr` →
  `strlen`, truncating at the first embedded NUL. The guard now uses
  `offsetof(MireManagedString, data)`. `in::all()` + `str::len` now report
  exact lengths and byte-exact round-trips pass (`tests/std_io.sh`: 22/22).
- **Native C compilation migrated from clang to tcc/libtcc.** `precompile_c_object`
  and the native linker now use `tcc` (discovered via `$TCC_PATH`, PATH, or
  standard locations) instead of `clang`. Runtime C sources (`__int128`) are
  guarded with `i128_t`/`u128_t` typedefs for tcc compatibility. The
  clang→tcc transition removes the clang dependency from the C object
  compilation pipeline; LLVM/Clang remain required only for the LLVM IR
  pipeline (`opt`, `llc`, `lld`).
- **IR cache keyed on codegen epoch.** The incremental MIR→IR cache now
  includes a fingerprint of `src/compiler/mir` sources (emitted via
  `MIRE_CODEGEN_EPOCH` from build.rs). This prevents a codegen edit from
  silently reusing IR produced by the previous compiler. The epoch is
  overridable with `MIRE_CACHE_EPOCH`.
- **install.sh: package manager + AUR + compiler validation.** `deps_for_pm()`
  lists gcc as non‑optional (Avenys compiles its own PAL/runtime in C).
  `install_deps()` adds error capture, AUR tcc installation when a helper is
  present (paru/yay/pikaur/trizen), and a post‑install check that a C compiler
  is on PATH.

## Important branch note

This README documents the `opt/vec-perf` working branch. The `main` branch has
advanced to **V4.4.16** (inline bump allocator work), which is tracked in a
separate open issue (`mire-lang/Avenys-rust#34`) covering several confirmed
defects (2 GiB bump arena SIGSEGV, stale IR cache, vector free list
corruption, enum address comparison, struct under-allocation, `strlen`-based
minimal strings, FFI `ptr` coercions, `-l:` with tcc, and CI gaps). Those
defects are **not** present in this branch; they belong to `main`/V4.4.16.

## Important: Always Use owl CLI

**Always use `owl` CLI (package manager). Never use `mire` CLI directly except for compiler development.**

| Task | Use this | NOT this |
|------|----------|----------|
| Create project | `owl new myproject` | - |
| Build | `owl build` | `mire build` |
| Run | `owl run` | `mire run` |
| Test | `owl test` | `mire test` |
| Check | `owl check` | `mire check` |
| Debug | `owl debug` | `mire debug` |
| Dependencies | `owl load <name>` | - |
| Package install | `owl install <name>` | - |

## Installation

The install script supports modular installation of the Mire toolchain components:
- **owl** - package manager
- **mire** - compiler (Avenys)
- **kioto** - standard library

All install methods use the same script from `install/install.sh`.

### Linux (x86_64, aarch64, riscv64)

#### Option 1: Quick full install (recommended)

```bash
# Installs: owl + kioto + mire compiler
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --compiler
```

#### Option 2: Auditable install

```bash
# Review the script before running
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh -o install.sh
less install.sh
chmod +x install.sh && ./install.sh --compiler
```

#### Option 3: User-local install (no sudo)

```bash
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --prefix ~/.local --compiler
```

### Architecture detection and selection

The installer picks a release triple from `uname -m` automatically. Override
it with `--arch <triple>` (or the `MIRE_ARCH` environment variable). The
supported triples are printed at the prompt, and an unsupported triple aborts
before anything is downloaded or installed:

```bash
# Force a concrete release architecture
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh \
  | sh -s -- --compiler --arch aarch64-unknown-linux-gnu
```

The host toolchain always runs the native toolchain; `--arch` only selects
which release archive to install.

### Audit before installing (does not touch the system)

`--check` validates the host against the release requirements and prints an
audit report, then exits 0 when everything is present and 1 otherwise
(never installs packages, never downloads):

```bash
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --check
```

### Build the compiler from source

When a release binary does not match the host (older glibc, different libc
variant, custom LLVM, or simply no prebuilt archive for the architecture),
compile the compiler against the host instead:

```bash
# Fetches avenys-rust @ main, ensures rustc >= 1.85, builds with the local
# LLVM (>= 22), installs the compiler. owl/kioto still come from the release.
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh \
  | sh -s -- --build-from-source
# SOURCE_URL / SOURCE_REF override the source location and ref; MIRE_RUSTUP_URL
# overrides the rustup installer URL.
```

### Docker fallback

If the toolchain container is available for the host architecture, install
inside it instead of the host:

```bash
# Pulls mire-lang/toolchain:<arch> and runs the install inside the container.
# Set MIRE_DOCKER_IMAGE to select another image.
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --docker
```

### Install options

```bash
# Owl + Kioto stdlib (default)
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh

# Owl + Kioto + Mire compiler
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --compiler

# Mire compiler only (no owl, no kioto)
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --compiler-only

# Kioto stdlib only (sets up ~/.owl/modules/kioto/)
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --kioto-only

# Owl only (package manager, no kioto, no compiler)
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --owl-only

# Non-interactive
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --compiler --yes

# Specific versions (when release artifacts exist)
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh \
  | sh -s -- --tag-compiler v4.2.0 --tag-kioto v2.4.9

# Skip shell profile PATH modification
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --compiler --no-profile
```

### Release archives

Releases publish per-architecture archives named by release triple, with a
legacy fallback name for x86_64:

| Component | Canonical (per-release) | Legacy fallback |
| --- | --- | --- |
| Compiler | `mire-compiler-<triple>.tar.gz` | `mire-compiler-linux-x86_64.tar.gz` |
| Owl | `owl-<triple>.tar.gz` | `owl-linux-x86_64.tar.gz` |

`--arch` selects a canonical archive for that triple only. When no `--arch` is
given on x86_64, the installer tries the canonical x86_64 name first, then the
legacy name. Owl bundles its runtime `libsodium.so` under `owl/lib/`, which the
installer copies to `<prefix>/lib/mire/`.

### Prerequisites

The script installs these automatically via your package manager when
`--yes` is supplied (or asks for confirmation): `curl`, `tar`, `git`, Rust/
Cargo, Clang/LLVM and LLD, `pkg-config`, OpenSSL, libsodium, zlib, zstd, and
libarchive. SDL is a project dependency, not a compiler prerequisite.
`protobuf-compiler` is only used by repository CI to validate the checked-in
Owl/Avenys schema; neither the installer nor the runtime needs it. Compiler
release archives preserve the nested `runtime/` and `pal/` source trees, so
reorganizing runtime modules does not add a host package dependency.
`tcc` (or `libtcc-dev`) is required to compile the runtime C sources.

Or install them manually:

```bash
# Debian/Ubuntu
sudo apt install curl tar git clang llvm-dev lld cargo pkg-config libssl-dev libsodium-dev zlib1g-dev libzstd-dev libarchive-dev tcc libtcc-dev

# Arch Linux
sudo pacman -S curl tar git clang llvm lld rust openssl libsodium zlib zstd libarchive tcc

# Fedora/RHEL
sudo dnf install curl tar git clang llvm-devel lld rust cargo pkgconf-pkg-config openssl-devel libsodium-devel zlib-devel zstd-devel libarchive-devel tcc
```

### Post-install

```bash
# Restart your shell or source your profile
source ~/.bashrc # or ~/.zshrc

# Verify installation
owl --version
mire --version

# Create a new project
owl new myproject
cd myproject
owl info
owl run
```

### Library status() function

Every official Mire library (mire, kioto, sdl, sqlite, wasm) exposes a
`pub fn status: () :str` function that returns a phpinfo-style console report.
Use it to inspect what a loaded library provides:

```bash
# In any Mire program that loads a library:
use dasu(mire::status())
use dasu(kioto::status())
use dasu(sdl::status())
use dasu(sqlite::status())
```

Or from the shell via `owl`:

```bash
# After loading a library in a project, export will verify status():
owl export          # warns if status() is missing (does not block)
owl export -e       # same as above (short flag)
```

### MOR registry (Mire Official Registry)

The official package registry lives at `mire-lang/libs`. It tracks package
versions, default symlinks, and install paths.

| Package | Versions | Default | Symlink |
|---------|----------|---------|---------|
| kioto | 2.5.0, 3.1.0 | 3.1.0 | `kioto -> kioto@3.1.0` |
| mire | 1.2.4, 2.0.0-beta | 2.0.0-beta | `mire -> mire@2.0.0-beta` |
| sdl | 1.0.1 | 1.0.1 | - |
| sqlite | 0.1.0 | 0.1.0 | - |
| wasm | 0.2.0 | 0.2.0 | - |
| avenys | 4.4.13 | 4.4.13 | - |
| owl | 1.3.0 | 1.3.0 | - |

Multi-version packages use version-suffixed directories:
`~/.owl/libs/kioto@3.1.0`, `~/.owl/libs/mire@2.0.0-beta`, etc.
The installer creates default symlinks automatically.

---

## Quick start (owl CLI)

### Using owl (recommended for all day-to-day work)

```bash
# Create a new project
owl new myproject
cd myproject
owl run

# Build, run, test
owl build
owl run
owl test
owl check
owl debug
```

### Compiler development (build from source)

```bash
# Clone and build the compiler
git clone https://github.com/mire-lang/Avenys-rust
cd Avenys-rust
cargo build --release

# Run tests
cargo test --release

# Install locally (optional)
cp target/release/mire ~/.local/bin/mire
mire --version
```

### Using mire directly (compiler development only)

```bash
# Build and run a program
mire build hello.mire
mire run hello.mire

# Test
mire test
mire check hello.mire
mire debug hello.mire --ir
```

---

## CLI boundary: compiler versus Owl

Avenys exposes only the compiler operations `build`, `run`, `test`, and
`debug`. Project creation, dependency installation, registries, lockfiles,
checks, and upgrades belong to Owl. Owl resolves dependencies first and passes
their directories explicitly to Avenys with `--lib-dir`.

Every compiler command that consumes a source file accepts an explicit input
file. When no output or cache path is supplied, standalone compilation uses
`<source-directory>/bin/{debug,release}` and `<source-directory>/bin/.cache`;
project builds use the project's `bin/` directory.

Source files may use either `.mire` or `.mr`; source discovery, local module
loads, package exports and the test harness treat both extensions equivalently.

```bash
mire build src/main.mire \
  --lib-dir ~/.owl/libs \
  --output-dir bin/release \
  --cache-dir bin/.cache \
  --target x86_64-unknown-linux-gnu \
  --runtime minimal
```

Relevant path flags are:

- `--lib-dir <path>`: explicit package directory; repeat through a
  colon-separated list when Owl has several resolved dependency roots.
- `-o, --output <file>`: exact executable or library output.
- `--output-dir <dir>`: output directory when the filename should be inferred.
- `--cache-dir <dir>` (alias `--cache`): incremental cache location.

The native C-object cache is stored below `<cache-dir>/cobjects` when this
flag is used, so a build can be relocated or cleaned as one unit.

These paths are compiler inputs, not dependency management. Avenys does not
install packages, consult registries, or choose a project entry from a
manifest.

```bash
mire build --config .mire-config.toml code/main.mire
```

Owl passes a generated configuration with `--config`; standalone compiler
invocations must provide their input and build paths explicitly.

---

## How it works

Every Mire program passes through the following pipeline:

```
Source (.mire)
  |
  v
Lexer -> Parser -> Type checker -> Borrow checker
  |
  v
MIR lowering -> MIR optimization (to fixed point)
  |
  v
LLVM IR generation -> opt (O0-O3) -> LLVM object/archive tools -> artifact
```

**Stage 1 - Frontend:** The lexer tokenizes UTF-8 source, the parser builds an AST. The type checker infers and verifies every expression with real fixed-width types (i8..i128, u8..u128, f32/f64, bool, char, str). The borrow checker enforces ownership: no use-after-move, no mutation during shared borrows, no dangling references.

**Stage 2 - MIR:** The AST lowers to a Mid-level Intermediate Representation. Optimization passes run to fixed point: constant folding, copy propagation, dead code elimination, branch folding, block merging, inlining, strength reduction, and more. New in v3.24+: division/remainder inlining (`sdiv`/`srem`), bounds-check inlining, string concat flattening (`MirOp::Concat`), and `MirOp::Drop` for explicit resource management.

**Stage 3 - Codegen:** MIR translates to LLVM IR text. The compiler invokes LLVM's `opt` for further optimization (O1-O3/Os/Oz), then `llc` lowers Mire IR to objects. Static Mire libraries use `llvm-ar`; PAL/runtime C objects remain compiled by the configured C compiler (tcc by default on this branch). Runtime tier controls what's linked:
- `minimal` (default): demand-driven - only used `rt_*`/`pal_*` symbols
- `none`: freestanding - no runtime/PAL, user provides `_start`

The legacy setting `runtime = "full"` is accepted as an alias for `minimal`;
there is no separate full-runtime build tier.

For `wasm32-wasip1`, Avenys uses the WASI SDK when `WASI_SDK_PATH` or
`WASI_SYSROOT` is set. `wasm32-unknown-unknown` is a no-libc freestanding
target; host services must be supplied as imports.

**Incremental compilation:** On rebuild, a fingerprint (source hash + dependency graph) is checked. If unchanged, returns in single-digit milliseconds. On partial changes, only affected units are re-analyzed. Cache uses WAL (write-ahead log) for crash-safe persistence.

---

## Shared libraries

```bash
# Build a shared library (.so)
owl build --crate-type cdylib

# Build a static library (.a)
owl build --crate-type staticlib
```

Or configure in `owl.toml`:

```toml
[build]
crate-type = "cdylib"   # or "staticlib"
```

Produces `.so` (shared) or `.a` (static) libraries with all symbols exported for `dlopen`/`dlsym` use.

```bash
mire build code/main.mire --artifact shared --output lib/libdemo.so
```

### Debug CLI Help

```bash
mire debug --help
```

Shows all debug options: `--tokens`, `--ast`, `--ir`, `--run`, plus build profiles and output options.

---

## The Mire Language

[Full syntax reference ->](./SYNTAX.md)

---

## The Language at a Glance

```mire
// Functions with inferred or explicit return types
fn fib: (n: i64) :i64 {
  if n <= 1 { return n }
  return fib(n - 1) + fib(n - 2)
}

// Structs and methods
struct Point { x: i64, y: i64 }

impl Point {
  fn dist: (self) :f64 {
    return sqrt((self.x * self.x + self.y * self.y) :f64)
  }
}

// Struct inheritance (extends)
pub struct Animal { name: str }
pub struct Dog extends Animal { breed: str }

impl Dog {
  fn greet: (self) :str {
    return self.name + " the " + self.breed
  }
}

// Skills (traits) with inheritance (super)
pub skill Greeter { fn greet: (self) :str }
pub skill Named super Greeter { fn get_name: (self) :str }

// Enums with pattern matching
enum Option[T] { None, Some(value: T) }

pub fn main: () {
  set p = Point::new(3, 4)
  set d = p.dist()
  use dasu("Distance: {d}")

  set dog = (Dog name: "Rex" breed: "Husky")
  use dasu(dog.greet())
}
```

[Full syntax reference ->](./SYNTAX.md)

---

## Project structure

```
avenys/
├── src/
│ ├── lexer/        # UTF-8 source scanning and tokenization
│ ├── parser/       # Recursive descent parser and AST
│ ├── compiler/     # Type checker, borrow checker, semantic analysis
│ │ └── mir/        # MIR lowering, optimization, and LLVM codegen
│ ├── avens/        # Build pipeline, codegen, CLI integration
│ ├── incremental/  # Incremental cache (LRU, WAL, fingerprinting)
│ ├── loader/       # Module resolution and symbol renaming
│ └── pal/          # PAL ABI, core dispatch, and Linux host adapter
├── install/        # Installation script
├── tests/          # Integration tests + compiler benchmarks
├── docs/           # CHANGELOG, error codes, architecture docs
├── examples/       # Example programs
├── bench/          # 21 kernel benchmark suite (Mire / Rust / C)
└── SYNTAX.md       # Complete language reference
```

---

## Standard library (Kioto)

Kioto lives at `~/.owl/libs/kioto/` and provides:

| Module | What it does |
|--------|-------------|
| `strings` | upper/lower, split/join, replace, trim, pad, substr, from, copy, repeat |
| `math` | trig, log, powers, statistics, random, complex numbers, decimal (basic, stats, complex, decimal, random) |
| `fs` | read, write, exists, mkdir, remove/remove_all, is_file, path/root/dir/file handles, last_error |
| `env` | var, cwd, args |
| `proc` | create, spawn (argv-safe, no shell), output_cwd, wait, kill, stdio channels, last_exit, read_line |
| `async` | channel (send/recv/close), task (ready/value), spawn/wait |
| `time` | now::ms/now::ns, elapsed/mark, sleep |
| `mem` / `cpu` | system resource queries |
| `net` | socket (connect/send/recv/close), listener (bind/accept/close) |
| `log` / `cli` | logging and CLI parsing |
| `crypto` | SHA-256/512, hex/base64, CSPRNG, Ed25519 |
| `sdl3` | joystick, gamepad, haptic, sensor, render, font_ttf, events, structs |

`fs::remove` removes a single entry (file, symlink, or empty dir) without ever
following symlinks; `fs::remove_all` recursively removes a tree (still
symlink-safe - external targets are never entered). On failure,
`fs::last_error()` returns the PAL error code (`11` = directory not empty).
See `kioto/README.md` for examples.

Dynamic collections (`vec`, `map`) are provided by the `mire` standard library,
loaded with `load mire::vec` / `load mire::map`. See `mire/README.md`.

### Compatible beta libraries (verified with Avenys v4.4.9-beta)

The following library betas are verified to load and link against this branch
of Avenys (`opt/vec-perf` / commit `a957435`). They are published to the
Mire Official Registry (MOR) under `registry/mor/index.toml`.

| Library | Version | Status | Repository | Commit | Notes |
|---------|---------|--------|------------|--------|-------|
| `mire` | `2.0.0-beta` | beta | https://github.com/mire-lang/mire-lib.git | `e2f5542` | PAL bindings, nested namespaces, `cons` stream selectors, `vec::binary::search`. Load with `load mire::vec` / `load mire::std` / `load mire::str`. |
| `kioto` | `3.0.0-beta` | beta | https://github.com/Evelynx-Dev/Kioto-1.git | `ddc2063` | Next-generation stdlib (beta). Follows `kioto 2.5.0`. |
| `kioto` | `2.5.0` | beta | https://github.com/Evelynx-Dev/Kioto-1.git | `a44b141` | Stable 2.x line, namespaced net API. |

Install via Owl:

```toml
# owl.toml
[dependencies]
    mire = "2.0.0-beta"
    kioto = "3.0.0-beta"
```

```mire
load mire::vec
load kioto::strings

pub fn main: () {
    set v = vec::with::capacity::i64(4) mut
    set v = vec::push::i64(v 42)
    use dasu(str::upper("hello"))
}
```

### Runtime tiers

The `[c] runtime` setting in `owl.toml` controls what gets linked:

| Tier | C files | Libraries | Use case |
|------|---------|-----------|----------|
| `minimal` (default) | demand-driven | `-lm -lc` (+ crypto if PAL used) | Standard builds, embeddings |
| `none` | 0 | none (static) | Freestanding, kernels, bare metal |

The legacy setting `runtime = "full"` is accepted as an alias for `minimal`;
there is no separate full-runtime build tier.

With `runtime = "none"` + `nostartfiles = true` + `nostdlib = true` you get a
~14KB statically linked binary with **zero external dependencies**.

### Load vs load!: module call syntax (E0025 / E0026)

Two error codes enforce correct module call syntax:

| Code | Trigger | Fix |
|------|---------|-----|
| **E0025** | Using `use!` with `load` (package) modules | Call directly: `str::from_i64(42)` |
| **E0026** | Calling `load!` modules without `use!` | Wrap: `use! math::add(1, 2)` |

```mire
// Package load (owl.toml dependency) - direct calls
load mire::str
pub fn main: () {
  set x = str::from_i64(42)  // OK direct call
  // set x = use! str::from_i64(42)  // E0025 forbidden
}

// Local load! - mandatory use!
load! /math
pub fn main: () {
  set x = use! math::add(1, 2)  // OK mandatory use!
  // set x = math::add(1, 2)  // E0026 rejected
}
```

---

## CLI

```bash
# Use owl for all project work
owl build [--release] [-O<0-3|s|z>]
owl run [FILE] [--release] [-O<0-3|s|z>] [-- <args>]
owl test [--verbose] [--no-run] [-j N]
owl debug [FILE] [--tokens] [--ast] [--ir] [--run]

# Compiler development only (use mire directly)
# mire build [file] [--release] [-O<0-3|s|z>]
# mire run [file] [--release] [-O<0-3|s|z>]
# mire debug [file] [--tokens] [--ast] [--ir]
# mire test [paths...] [--no-run] [--verbose] [--show-warn] [-O<0-3|s|z>] [-r] [-d]

# Native compiler link/runtime controls are available on build/run/debug:
# --target <triple>  -L, --link <dir>  -l, --link-lib <name>
# --artifact <bin|static|shared>  --runtime <minimal|none>
```

Warnings are **off by default**. Enable with:
- `--show-warn` - show all warnings (summary)
- `--position` - show per-file warning locations
- `-W <code>` - promote a specific warning to error (e.g. `-W W0001`)
- `--deny <code>` - deny a specific warning code
- `--warnings-as-errors` - deny all default warnings (W0001-W0005, W0034, W0039)
- `--no-warn <category>` - suppress warnings by category (e.g. `--no-warn Unused`)

---

## Documentation

> Modularized reference. Each section has its own README.md with comprehensive coverage.

### Language Syntax

All language syntax topics are organized under [docs/syntax/](docs/syntax/README.md) with a [main index](docs/syntax/README.md) for navigation:

| Section | Description |
|---------|-------------|
| [Program Structure](docs/syntax/program-structure/README.md) | Entry point, comments, attributes, modules |
| [Variables](docs/syntax/variables/README.md) | Declaration, mutability, constants, scope |
| [Types](docs/syntax/types/README.md) | Primitive, composite, type ascription, conversion |
| [Functions](docs/syntax/fn/README.md) | Declaration, calls, methods, generics, closures |
| [Control Flow](docs/syntax/control-flow/README.md) | if/else, loops, match, break/continue |
| [Pattern Matching](docs/syntax/pattern-matching/README.md) | match expressions and statements |
| [Structs & Inheritance](docs/syntax/poo/README.md) | Structs, methods, `extends`, `impl` |
| [Enums](docs/syntax/enums/README.md) | Enum declarations, payloads, matching |
| [Skills](docs/syntax/skills/README.md) | Traits, `super` inheritance, generic bounds |
| [Generics](docs/syntax/generics/README.md) | Generic functions, structs, trait bounds |
| [Collections](docs/syntax/collections/README.md) | vec, map, array, Box |
| [Strings](docs/syntax/strings/README.md) | String operations and methods |
| [Operators](docs/syntax/operators/README.md) | Arithmetic, comparison, bitwise, pipeline |
| [Modules](docs/syntax/modules/README.md) | load, load!, exports, namespace access |
| [FFI](docs/syntax/ffi/README.md) | C external function interface |
| [Macros](docs/syntax/macros/README.md) | Macro system, `@[macro!]`, hygiene |
| [Error Handling](docs/syntax/error-handling/README.md) | Result, Maybe, panic, `?` operator |
| [Testing](docs/syntax/testing/README.md) | `@[test]`, test framework, assertions |
| [Builtins I/O](docs/syntax/builtins-io/README.md) | dasu, ireru, proc I/O |
| [Pipeline](docs/syntax/pipeline/README.md) | Pipeline operators `=>` and `?=>` |
| [Memory Ownership](docs/syntax/memory-ownership/README.md) | Move, borrow, ownership semantics |

Full language reference: [SYNTAX.md](./SYNTAX.md)

### Compiler, ABI & Runtime

| Document | Description |
|----------|-------------|
| [Compiler Architecture](docs/compiler/README.md) | Pipeline stages, internals, configuration |
| [Command Line Interface](docs/cli/README.md) | Compiler flags and build paths |
| [Mire ABI v4](docs/abi/README.md) | ABI specification, type layout, calling convention |
| [Runtime](docs/rt/README.md) | Tier system (full/minimal/none), symbol reference |
| [PAL v4](docs/pal/README.md) | Platform Abstraction Layer with tests |
| [Error Codes](docs/errors/README.md) | All compiler and runtime error codes |

### Platform, Libraries & Help

| Document | Description |
|----------|-------------|
| [WebAssembly & WASI](docs/wasm/README.md) | WASM targets, host imports |
| [Building Libraries](docs/lib/README.md) | Library types, structure, exports, Owl integration |
| [FAQ](docs/faq/README.md) | Frequently asked questions |
| [Changelog](CHANGELOG.md) | Version history |

---

## Documentation Index

For the full modularized documentation, see [docs/README.md](docs/README.md).

---

## Benchmark suite

The `bench/` directory holds 21 kernels, each implemented in Mire, Rust, and C
performing the same calculation. `./bench/run.sh verify` compares output before
`./bench/run.sh measure` reports timings.

| Kernel | Workload |
|--------|----------|
| `alloc_pressure` | Eight million short-lived vectors, each pushed eight times |
| `binary_search` | Repeated binary search probes on a sorted vector |
| `break_continue` | Branch-heavy loop control |
| `list_indexed_read` | Repeated indexed reads from a vector |
| `many_locals` | Live local variables and register pressure |
| `matrix_rowmajor` | Row-major fill, column-major sum (stride access) |
| `method_dispatch` | Struct method dispatch |
| `mixed_int_float` | Interleaved integer and floating-point operations |
| `nested_loops` | Two nested loops |
| `numeric_format` | `str::from::i64` / `str::to::i64` round-trip |
| `recursion_ackermann` | Recursive Ackermann workload |
| `sort_introsort` | Insertion sort over 2000 elements |
| `string_impl` | Method delegation through an implementation |
| `string_scan` | Repeated `str::index` / `str::substr` over text |
| `swap_exchange` | Value swapping pattern |
| `trait_dispatch` | Trait/skill method dispatch |
| `vec_growth` | Vector growth from empty to 2000 elements |
| `vec_mixed_reads` | Mixed vector read patterns |
| `vec_partition` | Partition a vector into even/odd |
| `vec_pop_drain` | Build then drain with pop |
| `while_counter` | Simple counted while loop |

See `bench/README.md` for the full table, methodology, and measurement caveats
(noise floor below ~25 ms where startup dominates).

---

## ABI Verification

The PAL ABI v4 is validated through:
- `cargo test --release --test abi_consistency` - 8/8 pass
- `cargo test --release --test pal_conformance` - 8/8 pass
- Symbol count: 126 symbols (128 compiler-emitted minus 2 struct-return)
- All documented symbols match compiler emission and C implementation

---

## License

GNU General Public License v3.0

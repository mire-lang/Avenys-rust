# Avenys v4.2.0

**A compiled, ownership-aware systems language with an LLVM backend.**

Avenys is a statically typed programming language designed for clarity and control.
It gives you structs, enums, generics, closures, pattern matching, and a borrow
checker that tracks ownership at compile time - no garbage collector, no runtime
overhead beyond what you ask for.

The compiler, Avenys, translates Mire source through a multi-stage pipeline into
native binaries via LLVM. It ships with Kioto, a standard library covering
strings, collections, math, filesystem, processes, and more.

## Recent Changes (v4.3.0)
- **`bits::<T>(x)` bit-level reinterpretation**: a same-width scalar bitcast
  lowered to a native LLVM `bitcast`, with no runtime call and no C builtin.
  Useful for IEEE-754 payload work, where a NaN's bits or a float's exact
  representation matter and there is no arithmetic that would preserve them.
  The type checker rejects a non-scalar source, a non-bitcastable target and
  any width mismatch, pointing at the call site.
- **Float division no longer folds to `0.0`**: constant folding erased a
  division between two literals (`1.0 / 2.0` became `0`), because the
  `SDiv` folding arm ignored its operands. It now folds to a real division.
- **`mire test` is independent of the artifact**: a test always builds an
  executable, whatever `[build] artifact` declares, and always gets at least the
  minimal runtime. Libraries used to produce a shared object with no test entry
  point, so every test file came back `ok` having run nothing. A failing test
  file that declares no `@[test]` also now counts as at least one failure
  instead of being invisible in the totals.
- **Multi-arch installer**: `install/install.sh` detects the release triple
  (`--arch` overrides), installs per-arch canonical archives, and adds
  `--check` (read-only audit), `--build-from-source` (host LLVM build) and
  `--docker` (toolchain container) paths. Release pipeline builds x86_64 +
  aarch64 (+ riscv64 via QEMU, experimental) and publishes the toolchain
  container to GHCR.

See [CHANGELOG.md](CHANGELOG.md) for the full version history.

---

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

```text
  audit report
  ------------
  system : Ubuntu 24.04.4 LTS
  libc   : glibc 2.39
  arch   : x86_64-unknown-linux-gnu (detected)
  pkg mgr: apt
  llvm   : 18 (need >= 18 for compiles)

  prerequisites check complete - all present.
```

### Build the compiler from source

When a release binary does not match the host (older glibc, different libc
variant, custom LLVM, or simply no prebuilt archive for the architecture),
compile the compiler against the host instead:

```bash
# Fetches avenys-rust @ main, ensures rustc >= 1.85, builds with the local
# LLVM (>= 18), installs the compiler. owl/kioto still come from the release.
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
Cargo, Clang/LLVM, `pkg-config`, OpenSSL, libsodium, zlib and zstd. SDL is a
project dependency, not a compiler prerequisite.

Or install them manually:

```bash
# Debian/Ubuntu
sudo apt install curl tar git clang llvm-dev cargo pkg-config libssl-dev libsodium-dev zlib1g-dev libzstd-dev

# Arch Linux
sudo pacman -S curl tar git clang llvm rust openssl libsodium zlib zstd

# Fedora/RHEL
sudo dnf install curl tar git clang llvm-devel rust cargo pkgconf-pkg-config openssl-devel libsodium-devel zlib-devel zstd-devel
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
cargo test

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
  --runtime full
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

**Stage 3 - Codegen:** MIR translates to LLVM IR text. The compiler invokes LLVM's `opt` for further optimization (O1-O3/Os/Oz), then `llc` lowers Mire IR to objects. Static Mire libraries use `llvm-ar`; PAL/runtime C objects remain compiled by the configured C compiler. Clang is retained only as the target-aware CRT/libc/native-library link driver for executables. Runtime tier controls what's linked:
- `Full` (default): all runtime + PAL symbols
- `Minimal`: demand-driven - only used `rt_*`/`pal_*` symbols
- `None`: freestanding - no runtime/PAL, user provides `_start`

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

### Runtime tiers

The `[c] runtime` setting in `owl.toml` controls what gets linked:

| Tier | C files | Libraries | Use case |
|------|---------|-----------|----------|
| `full` (default) | 15 runtime + PAL | `-lm -lssl -lcrypto -lsodium -lc` | Full stdlib, backward compatible |
| `minimal` | demand-driven (1-6) | `-lm -lc` (+ crypto if PAL used) | Size optimization, embeddings |
| `none` | 0 | none (static) | Freestanding, kernels, bare metal |

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
# mire run [file] [--release] [-O<0-3|s|z>] [-- <args>]
# mire debug [file] [--tokens] [--ast] [--ir]
# mire test [paths...] [--no-run] [--verbose] [--show-warn] [-O<0-3|s|z>] [-r] [-d]

# Native compiler link/runtime controls are available on build/run/debug:
# --target <triple>  -L, --link <dir>  -l, --link-lib <name>
# --artifact <bin|static|shared>  --runtime <full|minimal|none>
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

## ABI Verification

The PAL ABI v4 is validated through:
- `cargo test --release --test abi_consistency` - 8/8 pass
- `cargo test --release --test pal_conformance` - 8/8 pass
- Symbol count: 126 symbols (128 compiler-emitted minus 2 struct-return)
- All documented symbols match compiler emission and C implementation

---

## License

GNU General Public License v3.0

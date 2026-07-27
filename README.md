# Avenys v3.24.5

**A compiled, ownership-aware systems language with an LLVM backend.**

Avenys is a statically typed programming language designed for clarity and control.
It gives you structs, enums, generics, closures, pattern matching, and a borrow
checker that tracks ownership at compile time — no garbage collector, no runtime
overhead beyond what you ask for.

The compiler, Avenys, translates Mire source through a multi-stage pipeline into
native binaries via LLVM. It ships with Kioto, a standard library covering
strings, collections, math, filesystem, processes, and more.

```
$ cat hello.mire
pub fn main: () {
 use dasu("Hello, world!")
}

$ mire run hello.mire
Hello, world!
```

---

## Install

The install script supports modular installation of the Mire toolchain components:
- **owl** — package manager
- **mire** — compiler (Avenys)
- **kioto** — standard library

All install methods use the same script from `install/install.sh`.

### Linux (x86_64)

#### Option 1: Quick full install (recommended)

```bash
# Installs: owl + kioto + mire compiler
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh
```

#### Option 2: Auditable install

```bash
# Review the script before running
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh -o install.sh
less install.sh
chmod +x install.sh && ./install.sh
```

#### Option 3: User-local install (no sudo)

```bash
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --prefix ~/.local
```

### Install options

```bash
# Owl only (package manager + kioto stdlib)
curl -fsSL ... | sh
curl -fsSL ... | sh -s -- # same as above, defaults to owl

# Owl + Mire compiler
curl -fsSL ... | sh -s -- --compiler

# Mire compiler only (no owl)
curl -fsSL ... | sh -s -- --compiler-only

# Kioto stdlib only (sets up owl home with kioto)
# Downloads and installs kioto to ~/.owl/modules/kioto/
curl -fsSL ... | sh -s -- --kioto-only
```

### Prerequisites

The script installs these automatically via your package manager:
- `curl`, `tar`, `clang`, `llvm`, `libssl`, `libsdl2`

Or install them manually:

```bash
# Debian/Ubuntu
sudo apt-get install curl tar clang llvm-dev libssl-dev libsdl2-dev

# Arch Linux
sudo pacman -S curl tar clang llvm openssl sdl2

# Fedora/RHEL
sudo dnf install curl tar clang llvm-devel openssl-devel SDL2-devel
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
owl run
```

## Quick start

```bash
# Build the compiler
cargo build --release

# Run the test suite
cargo test

# Compile and run a program
cargo run -- run examples/hello.mire
```

---

## How it works

Every Mire program passes through five stages:

```
Source (.mire)
 │
 ▼
 Lexer ──► Parser ──► Type checker ──► Borrow checker
 │
 ┌───────────────────────────────────────┘
 ▼
 MIR lowering ──► MIR optimization (9 passes to fixed point)
 │
 ▼
 LLVM IR generation ──► opt (O1-O3) ──► clang ──► Native binary
```

**Stage 1 — Frontend:** The lexer tokenizes, the parser builds an AST. The type
checker infers and verifies every expression. The borrow checker enforces
ownership rules: no use-after-move, no mutation during shared borrows, no
dangling references.

**Stage 2 — MIR:** The AST lowers to a Mid-level Intermediate Representation.
Nine optimization passes run to fixed point: constant folding, copy propagation,
dead code elimination, branch folding, block merging, inlining, and more.

**Stage 3 — Codegen:** MIR translates to LLVM IR text. The compiler invokes
LLVM's `opt` for further optimization (at O1+), then `clang` links the IR with
the C runtime and PAL objects into a native binary.

**Incremental compilation:** On your second build of the same source, the
compiler checks a fingerprint and returns in **single-digit milliseconds** if nothing
changed. On partial changes, only the affected units are re-analyzed.

---

## The language at a glance

```mire
# Functions with inferred or explicit return types
fn fib: (n: i64) :i64 {
 if n <= 1 { return n }
 return fib(n - 1) + fib(n - 2)
}

# Structs and methods
struct Point { x: i64, y: i64 }

impl Point {
 fn dist: (self) :f64 {
 return sqrt((self.x * self.x + self.y * self.y) :f64)
 }
}

# Enums with pattern matching
enum Option[T] { None, Some(value: T) }

pub fn main: () {
 set p = Point::new(3, 4)
 set d = p.dist()
 use dasu("Distance: {d}")
}
```

[Full syntax reference →](./SYNTAX.md)

---

## Project structure

```
avenys/
├── src/
│ ├── parser/ # Lexer + recursive descent parser
│ ├── compiler/ # Type checker, borrow checker, semantic analysis
│ │ └── mir/ # MIR lowering, optimization, and LLVM codegen
│ ├── avens/ # Build pipeline, codegen, CLI integration
│ ├── incremental/ # Incremental cache (LRU, WAL, fingerprinting)
│ ├── loader.rs # Module resolution (packages, imports, exports)
│ └── pal/ # Platform Abstraction Layer (Linux C backend)
├── install/ # Installation script
├── tests/ # Integration tests + compiler benchmarks
├── docs/ # CHANGELOG, error codes, architecture docs
└── SYNTAX.md # Complete language reference
```

---

## Standard library (Kioto)

Kioto lives at `~/.owl/modules/kioto/` and provides:

| Module | What it does |
|--------|-------------|
| `strings` | upper/lower, split/join, replace, trim, pad, substr |
| `lists` | push/pop/get, slice, concat, sort, reverse, unique |
| `dicts` | get/set/keys/values, has, remove, merge |
| `math` | trig, log, powers, statistics, random, complex numbers |
| `fs` | read, write, exists, copy, move, mkdir, list |
| `env` | get/set, args, cwd |
| `proc` | run, shell, spawn, pipe, signal handling |
| `task` | spawn, join, ready/failed checks |
| `time` | now, sleep, format, elapsed |
| `mem` / `cpu` | system resource queries |
| `maybe` / `result` | Option and Result types |

---

## CLI

```bash
mire run [file] [--release] [-O<0-3|s|z>] [-- args...]
mire build [file] [--release] [-O<0-3|s|z>]
mire check [file] [--show-warn] [-W <code>] [--deny <code>]
mire debug [file] [--tokens] [--ast] [--ir]
mire test [paths...] [--no-run] [--verbose] [--show-warn] [-O<0-3|s|z>] [-r] [-d]
```

Warnings are **off by default**. Enable with:
- `--show-warn` — show all warnings (summary)
- `--position` — show per-file warning locations
- `-W <code>` — promote a specific warning to error (e.g. `-W W0001`)
- `--deny <code>` — deny a specific warning code
- `--warnings-as-errors` — deny all default warnings (W0001–W0005, W0034, W0039)
- `--no-warn <category>` — suppress warnings by category (e.g. `--no-warn Unused`)

---

## Documentation

| Document | Description |
|----------|-------------|
| [SYNTAX.md](./SYNTAX.md) | Complete language reference with examples |
| [docs/FFI.md](./docs/FFI.md) | Foreign Function Interface reference |
| [docs/LIBRARIES.md](./docs/LIBRARIES.md) | Module loading, owl.toml, and exports |
| [docs/PAL-ABI.md](./docs/PAL-ABI.md) | Platform Abstraction Layer architecture |
| [docs/ERROR_CODES.md](./docs/ERROR_CODES.md) | All compiler error and warning codes |
| [docs/CHANGELOG.md](./docs/CHANGELOG.md) | Version history |
| [docs/mir-pipeline.md](./docs/mir-pipeline.md) | MIR design and optimization passes |
| [docs/incremental-design.md](./docs/incremental-design.md) | Cache architecture and fingerprinting |

---

## License

GNU General Public License v3.0

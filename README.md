# Avenys v4.4.13

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
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --mireup
```

### 2) Interactive installation (choose what to install)

```bash
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --interactive
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
curl -fsSL https://raw.githubusercontent.com/mire-lang/Avenys-rust/main/install/install.sh | sh -s -- --from-source
```

The installer auto-detects permissions - if run with `sudo` it installs to `/usr/local/bin/`, otherwise to `~/.local/bin/`. 
It always uses `owl install` for libraries and configures the MOR registry automatically.

**Supported architectures:** x86_64, aarch64 (riscv64 is compatible but must be built by hand).

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
- `mire@1.2.4`, `mire@2.0.0-beta` (default: 2.0.0-beta)
- `sdl@1.0.1`, `sqlite@0.1.0`, `wasm@0.2.0`

## License

GNU General Public License v3.0

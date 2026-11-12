# CLI - Compiler Command-Line Interface

> Avenys compiler commands, flags, and usage.

Version: **4.4.42**
Status: **Stable**
Date: **2026-11-11**

---

## Overview

The Avenys compiler provides a unified CLI via `mire` binary. All commands share common flags and support a consistent configuration model.

---

## Commands

### `mire build`

Compile a Mire program to a binary or library.

```bash
# Basic usage
mire build src/main.mire

# Release build with optimization
mire build src/main.mire --release -O3

# With explicit paths
mire build src/main.mire \
  --lib-dir ~/.owl/libs \
  --output-dir bin/release \
  --cache-dir bin/.cache \
  --target x86_64-unknown-linux-gnu \
  --runtime minimal

# Shared library
mire build src/main.mire --artifact shared --release -O3

# Static library
mire build src/main.mire --artifact static --release

# Build with config file
mire build --config .mire-config.toml src/main.mire

# Disable incremental cache
mire build --no-analysis-cache src/main.mire

# Verbose (shows all intermediate steps)
mire build src/main.mire --verbose
```

### `mire run`

Build and run in a single command.

```bash
mire run src/main.mire                    # Debug
mire run src/main.mire --release          # Release
mire run src/main.mire -- --arg1 value1  # Pass args to the program
mire run src/main.mire -O3 --release      # With optimization
```

### `mire test`

Run tests defined in the project.

```bash
mire test                          # All tests
mire test tests/specific.mire      # Specific file
mire test --verbose                # Verbose output
mire test --no-run                 # Compile only, don't execute
mire test -j 8                     # Parallel execution
mire test --log                    # Record metrics
```

### `mire check`

Analyze without building a binary.

```bash
mire check src/main.mire
```

### `mire debug`

Emit intermediate representations for debugging.

```bash
mire debug src/main.mire --tokens  # Token stream
mire debug src/main.mire --ast     # Abstract syntax tree
mire debug src/main.mire --ir      # LLVM IR
mire debug src/main.mire --run     # Run with debug output
```

---

## Global Flags

| Flag | Description |
|------|-------------|
| `--lib-dir <path>` | Package directory (repeatable, colon-separated) |
| `-o, --output <file>` | Exact output filename |
| `--output-dir <dir>` | Output directory |
| `--cache-dir <dir>` | Incremental cache location |
| `--target <triple>` | LLVM target triple |
| `--cpu <name>` | LLVM CPU for instruction selection (native, x86-64-v3, ...) |
| `--runtime <tier>` | minimal/none |
| `--artifact <bin\|static\|shared>` | Output type |
| `--release` | Release build |
| `-O, --opt-level <0-3\|s\|z>` | Optimization level |
| `--config <file>` | Config file |
| `--no-analysis-cache` | Disable analysis cache |
| `--verbose` | Verbose output |
| `--help` | Show help |
| `--version` | Show version |

### Native C Compilation (repeatable)

| Flag | Description |
|------|-------------|
| `--c-source <path>` | C source file to compile and link |
| `--c-include <dir>` | C include search directory |
| `--cflag <flag>` | Raw flag passed to the C compiler |
| `--c-lib <name>` | Native library to link |
| `--define <name[=val]>` | Preprocessor define |

### Security (the `[security]` section, without a config file)

| Flag | Description |
|------|-------------|
| `--security-mode <m>` | open \| strict |
| `--extern <pattern>` | Allowed extern symbol, `*` suffix allowed |
| `--extern-lib <name>` | Allowed extern library |
| `--allow-macro <name>` | Allowed macro |
| `--allow-unsafe` | Permit unsafe blocks |
| `--macro <name=path>` | Bind a macro name to a source file |

### Warnings (for build/run/test)

| Flag | Description |
|------|-------------|
| `--show-warn` | Show warning summary |
| `--position` | Show per-file warning locations |
| `--no-warn <cat>` | Suppress warning category (repeatable) |
| `-W <code>` | Promote warning to error |
| `--deny <code>` | Deny specific warning code |

---

## Config Files

### `.mire-config.toml` (normalized compiler config)

Owl generates this file at `bin/.cache/mire-config.toml` for builds and `bin/.cache/mire-test-config.toml` for tests. It contains the full resolved configuration: project metadata, build settings, paths, and dependency library directories.

```toml
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
```

### `owl.toml` (project config for Owl)

```toml
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
sources = "src"
tests = "tests"
output = "bin"
cache = "bin/.cache"
generated = "bin/generated"

[cfg]
libs = "~/.owl/libs"
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Compilation error |
| 2 | Usage error (invalid flags) |
| 3 | Linker error |
| 4 | Test failure |
| 124 | Timeout (when used with `timeout`) |

---

## Examples

### Minimal Freestanding Program

```toml
# owl.toml
[c]
runtime = "none"
nostartfiles = true
nostdlib = true
```

```bash
mire build src/main.mire --release -O3
```

Produces a **~15 KB binary** with no external dependencies.

### Library Build

```toml
# owl.toml
[build]
artifact = "cdylib"
```

```bash
mire build src/main.mire --release -O3 --artifact shared
```

Produces `libmain.so` (or `.dylib`/`.dll` on macOS/Windows).

### Cross-Compilation

```bash
mire build src/main.mire --target wasm32-wasip1 --release -O3
```

---

## Related

- [Compiler Internals](../compiler/README.md)
- [Configuration](../../README.md)
- [Avenys Documentation Index](../README.md)

---

## License

GNU General Public License v3.0

# CLI - Compiler Command-Line Interface

> Avenys compiler commands, flags, and usage.

Version: **4.1.0**
Status: **Stable**
Date: **2026-09-12**

---

## Overview

The Avenys compiler provides a unified CLI via `mire` binary. All commands share common flags and support a consistent configuration model.

---

## Commands

### `mire build`

Compile a Mire program to a binary or library.

```bash
# Basic usage
mire build src/main.mr

# Release build with optimization
mire build src/main.mr --release -O3

# With explicit paths
mire build src/main.mire \
  --lib-dir ~/.owl/libs \
  --output-dir bin/release \
  --cache-dir bin/.cache \
  --target x86_64-unknown-linux-gnu \
  --runtime minimal

# Shared library
mire build code/mod.mr --artifact shared --release -O3

# Static library
mire build code/mod.mr --artifact static --release

# Build with config file
mire build --config .mire-config.toml code/main.mr

# Disable incremental cache
mire build --no-analysis-cache src/main.mr

# Verbose (shows all intermediate steps)
mire build src/main.mr --verbose
```

### `mire run`

Build and run in a single command.

```bash
mire run src/main.mr                    # Debug
mire run src/main.mr --release          # Release
mire run src/main.mr -- --arg1 value1  # Pass args to the program
mire run src/main.mr -O3 --release      # With optimization
```

### `mire test`

Run tests defined in the project.

```bash
mire test                          # All tests
mire test tests/specific.mr      # Specific file
mire test --verbose                # Verbose output
mire test --no-run                 # Compile only, don't execute
mire test -j 8                     # Parallel execution
mire test --log                    # Record metrics
```

### `mire check`

Analyze without building a binary.

```bash
mire check src/main.mr
```

### `mire debug`

Emit intermediate representations for debugging.

```bash
mire debug src/main.mr --tokens  # Token stream
mire debug src/main.mr --ast     # Abstract syntax tree
mire debug src/main.mr --ir      # LLVM IR
mire debug src/main.mr --run     # Run with debug output
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
| `--runtime <tier>` | full/minimal/none |
| `--artifact <bin\|static\|shared>` | Output type |
| `--release` | Release build |
| `-O<0-3\|s\|z>` | Optimization level |
| `--artifact <type>` | Library type (legacy flag) |
| `--config <file>` | Config file |
| `--no-analysis-cache` | Disable analysis cache |
| `--verbose` | Verbose output |
| `--help` | Show help |
| `--version` | Show version |

---

## Build Pipeline Flags

| Flag | Stage | Description |
|------|-------|-------------|
| `--target` | Codegen | LLVM target triple |
| `--runtime` | Codegen | Tier gating (full/minimal/none) |
| `--artifact` | Linker | Output type |
| `--libt` | Linker | Legacy library type|
| `-O` | opt | Optimization level |
| `--lib-dir` | Loader | Package search paths |
| `--config` | Config | Configuration file |
| `--no-analysis-cache` | Cache | Disable incremental analysis |

---

## Config Files

### `.mire-config.toml`

```toml
[build]
libt = "cdylib"

[c]
runtime = "minimal"
target = "x86_64-unknown-linux-gnu"
```

### `owl.toml` (project config)

```toml
[project]
name = "myproject"
version = "0.1.0"
entry = "code/main.mr"

[build]
libt = "cdylib"
crate-type = "cdylib"

[c]
runtime = "full"
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
mire build src/main.mr --release -O3
```

Produces a **~15 KB binary** with no external dependencies.

### Library Build

```toml
# owl.toml
[build]
artifact = "cdylib"
```

```bash
mire build code/mod.mr --release -O3 --artifact shared
```

Produces `libmod.so` (or `.dylib`/`.dll` on macOS/Windows).

### Cross-Compilation

```bash
mire build src/main.mr --target wasm32-wasip1 --release -O3
```

---

## Related

- [Compiler Internals](../compiler/README.md)
- [Configuration](../../README.md)
- [Avenys Documentation Index](../README.md)

---

## License

GNU General Public License v3.0

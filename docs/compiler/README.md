# Compiler - CLI, Configuration, and Internals

> Avenys compiler command-line interface, configuration, and pipeline internals.

Version: **4.1.0**
Status: **Stable**
Date: **2026-09-12**

---

## CLI Interface

### `mire build`

```bash
# Standalone build
mire build src/main.mire --release -O3

# With explicit paths
mire build src/main.mire \
  --lib-dir ~/.owl/libs \
  --output-dir bin/release \
  --cache-dir bin/.cache \
  --target x86_64-unknown-linux-gnu \
  --runtime minimal

# With config file
mire build --config .mire-config.toml code/main.mire

# Build shared library
mire build code/main.mire --artifact shared --output lib/libdemo.so
```

### `mire run`

```bash
mire run src/main.mire --release -O3
mire run src/main.mire -- --arg1 value1  # Pass args to program
```

### `mire test`

```bash
mire test                          # Run all tests
mire test tests/specific.mire      # Run specific test file
mire test --verbose                # Verbose output
mire test --no-run                 # Compile only, don't run
mire test -j 8                     # Parallel test execution
mire test --log                    # Record metrics
```

### `mire check`

```bash
mire check src/main.mire           # Analyze only, no binary
```

### `mire debug`

```bash
mire debug src/main.mire --tokens  # Emit tokens
mire debug src/main.mire --ast     # Emit AST
mire debug src/main.mire --ir      # Emit LLVM IR
mire debug src/main.mire --run     # Run with debug output
```

### `mire build` flags

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
| `--libt <type>` | Library type (legacy) |
| `--config <file>` | Configuration file |
| `--no-analysis-cache` | Disable analysis cache |
| `--verbose` | Verbose output |

---

## Configuration (`owl.toml`)

```toml
[project]
name = "myproject"
version = "0.1.0"
entry = "code/main.mire"

[build]
libt = "cdylib"    # or "staticlib", "bin"
crate-type = "cdylib"   # hyphenated alias

[c]
runtime = "minimal"    # full | minimal | none
target = "x86_64-unknown-linux-gnu"
nostartfiles = false
nostdlib = false
libt = "bin"

[dependencies]
kioto = { path = "~/.owl/libs/kioto", version = "2.4.7" }
mire = { path = "~/.owl/libs/mire" }

[exports]
strings = "core/strings"
net = "core/net"

[macros]
assert = "core/macros/assert.mire"
```

---

## Build Pipeline Stages

```
Source (.mire)
    |
    v
Lexer -> Parser -> Type Checker -> Borrow Checker
    |
    v
MIR Lowering -> MIR Optimization (fixed point)
    |
    v
LLVM IR Generation -> Dependency Collection
    |
    v
opt (O0-O3) -> llc -> Object files
    |
    v
C Runtime (tier-aware) -> clang -> Artifact
```

### 1. Frontend

| Component | File | Responsibility |
|-----------|------|----------------|
| Lexer | `lexer/mod.rs` | UTF-8 scanning, tokenization |
| Parser | `parser/mod.rs` | Recursive descent, AST construction |
| Type Checker | `typeck.rs` | Type inference, validation |
| Borrow Checker | `borrowck/` | Ownership, moves, borrows |

### 2. MIR

| Module | Responsibility |
|--------|----------------|
| `lower/mod.rs` | Main lowering orchestration |
| `lower/expr.rs` | Expression lowering |
| `lower/stmt.rs` | Statement lowering |
| `lower/decl.rs` | Function/struct/enum lowering |
| `optimize/` | Fixed-point optimization passes |

### 3. Codegen

| Module | Responsibility |
|--------|----------------|
| `codegen/mod.rs` | `mir_to_llvm` entry |
| `codegen/expr.rs` | Instruction -> LLVM IR |
| `codegen/builtins.rs` | `pal_extern_decls()`, `builtin_to_pal` |
| `codegen/validate.rs` | Pre-codegen undefined-call check |
| `codegen/resolve.rs` | Symbol resolution |

### 4. Build Pipeline

| Module | Responsibility |
|--------|----------------|
| `avens/build_pipeline.rs` | Orchestrates full compilation |
| `avens/build_support.rs` | C file collection, dependency collector |
| `avens/config.rs` | Build configuration |
| `avens/toolchain.rs` | Linker and compiler flags |

### 5. Incremental Compilation

| Component | Responsibility |
|-----------|----------------|
| `cache.rs` | WAL-based cache, LRU, blob store |
| `hashing/` | Structural hashing of AST/MIR |
| `analysis.rs` | Analysis caching, invalidation |
| `dependencies.rs` | Dependency fingerprinting |

**Fingerprint**: source hash + dep graph + mode + opt + c_sources_hash

---

## Warning and Error Codes

### Error Codes (`E0xxx`)

| Code | Category | Description |
|------|----------|-------------|
| E0001 | Lexer | Unexpected token |
| E0003 | Parser | Syntax error |
| E0005 | Type | Type mismatch |
| E0007 | Ownership | Use after move |
| E0008 | Type | Cannot infer type |
| E0009 | Parser | Expected expression |
| E0010 | Type | Type mismatch |
| E0011 | Type | Invalid type |
| E0013 | Parser | Unterminated block |
| E0014 | Backend | Backend limitation |
| E0015 | Runtime | Runtime error |
| E0016 | Ownership | Borrow violation |
| E0017 | CLI | CLI error |
| E0018 | Parser | Ambiguous syntax |
| E0019 | Macro | Macro not allowlisted |
| E0020 | Macro | Invalid macro syntax |
| E0021 | Module | Unknown module |
| E0022 | Module | Duplicate definition |
| E0023 | Module | Circular dependency |
| E0024 | Module | Import error |
| E0100-E0110 | Type | Type width/cast errors |

### Warning Codes (`W0xxx`)

| Code | Category | Description |
|------|----------|-------------|
| W0001 | Unused | Unused variable |
| W0002 | Unused | Unused import |
| W0004 | Dead code | Unreachable code |
| W0005 | Type | Implicit type conversion |
| W0006 | Performance | Inefficient operation |
| W0007 | Style | Naming convention |
| W0008 | Style | Indentation |
| W0009 | Deprecated | Deprecated function |
| W0010 | Deprecated | Deprecated syntax |
| W0011 | Unused | Unused function |
| W0012 | Type | Shadowed variable |
| W0013 | Logic | Unreachable match arm |
| W0014 | Complexity | Nested complexity |
| W0017 | Memory | Unreleased resource |
| W0018 | Memory | Potential leak |
| W0019 | Style | Missing documentation |
| W0021 | Type | Trait bound warning |
| W0024 | Performance | Inefficient pattern |
| W0025 | Module | Circular dependency |
| W0034 | Deprecated | Deprecated function call |
| W0035 | Deprecated | Deprecated syntax |
| W0036 | Unused | Unused test |
| W0037 | Style | Naming inconsistency |
| W0038 | Logic | Duplicate match pattern |
| W0039 | Unused | Unused attribute |
| W0040 | Ownership | Potential ownership issue |
| W0041 | Ownership | Mutable borrow |
| W0042 | Memory | Potential memory issue |
| W0043 | Type | Implicit conversion |
| W0044 | Performance | Suboptimal pattern |
| W0045 | Style | Formatting |
| W0046 | Type | Trait implementation |
| W0047 | Logic | Unhandled case |
| W0048 | Memory | Resource cleanup |
| W0049 | Deprecated | Future removal |

---

## Help and Common Questions

### How to debug a compilation error?

```bash
mire debug src/main.mire --tokens   # See tokenization
mire debug src/main.mire --ast      # See AST structure
mire debug src/main.mire --ir       # See generated LLVM IR
```

### How to optimize for size?

```toml
[c]
runtime = "none"
nostartfiles = true
nostdlib = true
```

Or use `-Oz` optimization level.

### How to build a library?

```toml
[build]
libt = "cdylib"   # or "staticlib"
```

### How to use a custom target?

```bash
mire build --target wasm32-wasip1 --release
```

### How to pass arguments to a Mire program?

```bash
mire run src/main.mire -- arg1 arg2
```

### How to disable the incremental cache?

```bash
mire build --no-analysis-cache
```

### How to clean the build artifacts?

```bash
rm -rf bin/
```

### How to see what C files are compiled?

```bash
mire build --verbose 2>&1 | grep "\.c"
```

---

## Related

- [Mire ABI v4](../abi/README.md)
- [Runtime Configuration](../rt/README.md)
- [Libraries](../lib/README.md)
- [Avenys Documentation Index](../README.md)

---

## License

GNU General Public License v3.0

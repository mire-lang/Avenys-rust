# Mire

Mire is a compiled, statically typed programming language with ownership-oriented memory safety checks and an LLVM-based backend.

Current compiler version: **3.11.28**.

## Status

- **Active backend**: Avenys (MIR-based LLVM codegen)
- **Pipeline**: lexer → parser → type checker → semantic analysis → borrow checker → MIR lowering → MIR optimization → LLVM codegen
- **Incremental compilation**: enabled (cache with LRU pruning, reuse, analysis invalidation)
- **Optimization profiles**: `debug`/`release` + `-O0`/`-O1`/`-O2`/`-O3`/`-Os`/`-Oz`
- **MIR optimizations** (9 passes to fixed-point): constant folding, algebraic simplification, strength reduction, copy propagation, branch folding, dead code elimination, dead block elimination, block merging, inlining
- **Public CLI**: `build`, `run`, `check`, `debug`, `test`, `validate`, `owl add`, `owl remove`
- **Standard library (Kioto)**: modules for fs, env, strings, lists, dicts, time, cpu, mem, proc, async, math, term, gpu, types, maybe, result, tuple, iter
- **PAL (Platform Abstraction Layer)**: Linux backend; WASM backend planned
- **Runtime core**: `src/runtime/` — platform-independent managed strings, lists, dicts (C FFI)

## Quick Start

```bash
cargo build --release
cargo test
```

## CLI

```bash
mire build [file] [options]               # Compile to binary
mire run [file] [options] [-- args]        # Compile and run
mire check [file] [options]                # Type-check without codegen
mire debug [file] [options]                # Debug compilation
mire test [paths...] [options]             # Compile/run .mire tests
mire validate                              # Validate owl.toml
mire owl add <name> [--path] [--version]   # Add dependency
mire owl remove <name>                     # Remove dependency
```

Default profile is `debug` (`-O0`). Use `--release` or `-O2` for optimized builds.

## Documentation

- [Language Syntax](./SYNTAX.md)
- [PAL Architecture](./PAL.md)
- [Error & Warning Codes](./docs/ERROR_CODES.md)
- [Changelog](./docs/CHANGELOG.md)
- [MIR Pipeline](./docs/mir-pipeline.md)
- [Incremental Design](./docs/incremental-design.md)
- [Roadmap & Known Issues](./TODO.md)
- [ABI Map](./abi_map.toml)

## License

GNU General Public License v3.0

# Avenys Documentation

> Complete reference for the Avenys compiler, the Mire programming language, and the PAL/runtime infrastructure.

Version: **4.1.0**
Status: **Stable**

---

This directory contains all reference documentation for the Avenys/Mire toolchain. Every section is a self-contained directory with a `README.md`. Sections are listed alphabetically; each section links forward and backward to related documentation.

---

## Quick Navigation

| # | Section | What it covers |
|---|---------|----------------|
| 1 | [ABI](abi/README.md) | Mire ABI v4 specification |
| 2 | [CLI](cli/README.md) | Compiler command-line interface |
| 3 | [Compiler](compiler/README.md) | Pipeline internals, configuration |
| 4 | [Errors](errors/README.md) | All compiler error and warning codes |
| 5 | [FAQ](faq/README.md) | Frequently asked questions |
| 6 | [Libraries](lib/README.md) | Library creation, dependencies, Owl integration |
| 7 | [PAL](pal/README.md) | Platform Abstraction Layer with tests |
| 8 | [Runtime](rt/README.md) | Runtime tiers and verified symbol reference |
| 9 | [Syntax](syntax/README.md) | Language syntax topics |
| 10 | [WASM](wasm/README.md) | WebAssembly and WASI support |

---

## How to read this documentation

1. Start with the section that matches your need (see Quick Navigation above)
2. Each section's `README.md` has a **Related** link at the bottom pointing to connected sections
3. Within `syntax/`, each sub-topic has its own `README.md` indexed by a topic table
4. Use `../README.md` to go back to this index from any section

---

## Symbols verified against source

All runtime symbols documented in the [RT section](rt/README.md) are extracted directly from `src/runtime/runtime.h`. All PAL symbols are extracted from `src/pal/pal.h`. All error codes are extracted from `src/error/diagnostic.rs`. Every documented symbol exists in the compiled source.

---

## License

GNU General Public License v3.0

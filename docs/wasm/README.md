# WebAssembly and WASI Support

> Compiling Mire programs to WebAssembly via WASI Preview 1.

Version: **4.1.0**
Status: **Stable**
Date: **2026-09-12**

---

## Overview

Avenys supports compiling Mire programs to **WebAssembly (Wasm)** via the **WASI Preview 1** platform abstraction layer. This enables Mire programs to run in any WASI-compatible runtime (Wasmtime, Wasmer, Node.js, etc.).

The WASI PAL (`pal/wasi/pal_wasi.c`) provides stateless WASI system calls while keeping resource-bearing operations for future extension.

---

## Target Triple

```bash
mire build src/main.mire --target wasm32-wasip1 --release -O3
```

The `wasm32-wasip1` triple activates the WASI platform directory and WASI-specific linker flags.

---

## WASI Platform

### Stateless Services

The WASI PAL implements stateless services only (no handle-based lifecycle):

| Service | WASI Call | Mire PAL Equivalent |
|---------|-----------|---------------------|
| Print stderr | `fd_write(2, ...)` | `pal_io_print_err` |
| Time | `clock_time_get` | `pal_time_now_ms` / `pal_time_now_ns` |
| Random | `random_get` | `pal_crypto_random_bytes` |

### Resource-Bearing Operations

Resource-bearing PAL operations (file handles, sockets, processes, channels) are **not yet implemented** in the WASI PAL. Attempting these returns a stable error rather than a host fallback.

This design ensures safe, deterministic behavior in sandboxed WASI environments.

---

## Compilation Flow

```
Source ---> LLVM IR (@target wasm32-wasip1)

opt (O0-O3) --> llc -> .wasm

wasm-ld --> .wasm (or wasm-ld --target=wasm32-wasip1)
```

### Linker

The WASI target uses `wasm-ld` with WASI imports:
- `wasi_snapshot_preview1` module imported for `fd_write`, `clock_time_get`, `random_get`
- No dynamic linking - all WASI functions are imported by name

---

## Runtime Behavior

### Memory Model

- Linear memory (`i64` address space) - no GC in Wasm
- Mire managed strings are allocated via `malloc` -> freed via `free`
- No `weak` or `extern_weak` references (Wasm limitation)

### Imports

The generated Wasm imports from `wasi_snapshot_preview1`:
- `fd_write` - stderr output
- `clock_time_get` - high-resolution time
- `random_get` - cryptographically secure random

### Exports

The Wasm binary exports:
- `_start` - WASI entry point
- `_initialize` - WASI initialization (if applicable)
- All public Mire functions as `@_name` symbols

---

## Limitations

| Feature | Status |
|---------|--------|
| WASI Preview 1 | Stable |
| WASI Preview 2 | Not yet |
| File I/O | Not yet (resource-bearing) |
| Networking | Not yet |
| Process spawning | Not yet |
| Threads | Not yet (Wasm threads proposal) |
| Dynamic linking | Not yet |
| GC / GC pointers | Not yet |
| Multiple memories | Not yet |

---

## Running Wasm

### Wasmtime

```bash
wasmtime bin/release/main.wasm
```

### Wasmer

```bash
wasmer run bin/release/main.wasm
```

### Node.js

```bash
node bin/release/main.wasm
```

---

## PAL vs WASI

| Aspect | Native (Linux) | WASI |
|--------|----------------|------|
| File I/O | `openat` / `read` / `write` | `fd_read` / `fd_write` |
| Process | `fork` / `execve` | Not yet |
| Sockets | `socket` / `connect` | Not yet |
| Random | `/dev/urandom` | `random_get` |
| Time | `clock_gettime` | `clock_time_get` |
| Threads | `pthread` | Not yet |
| Environment | `environ` | Not yet |
| Handles | `pal_root_t` | Not yet |

---

## Related

- [PAL v4](../pal/README.md)
- [Compiler CLI](../cli/README.md)
- [Runtime Configuration](../rt/README.md)
- [Avenys Documentation Index](../README.md)

---

## License

GNU General Public License v3.0

# PAL Core — Implementation Guide

PAL Core implements the runtime infrastructure shared by all Host backends. It provides handle management, error state, memory allocation, ownership tracking, and validation. The Host Adapter (Linux, Windows, etc.) calls into PAL Core but never reimplements it.

## Architecture

```
PAL ABI (pal.h) ← the stable contract
       │
       ▼
PAL Core (src/pal/core/) ← handles, errors, allocators, validation
       │
       ▼
Host Adapter (src/pal/linux/) ← syscalls only, translates to ABI
```

PAL Core owns:
- Handle slot tables and generation counters
- Thread-local error state
- PAL-allocated memory pool
- Handle validity checks
- Ownership tracking
- Backend dispatch registry

## Handle Management

Handles are opaque identifiers composed of:
- Index: slot in the PAL Core table
- Generation: incremented on release

This prevents use-after-close and double-free. When a handle is closed, its generation increments. Any stale handle carries the old generation and is rejected.

PAL Core manages the slot table internally. The ABI defines only opaque handle types. No code outside PAL Core accesses the slot table.

## Error State

Thread-local error state is the only mutable global in PAL Core. It stores:
- Current error code (pal_error_code_t)
- Error message (static string, set by last operation)

PAL Core provides:
- `pal_set_error(code, message)` — internal use
- `pal_last_error()` — returns error code for Kioto
- `pal_strerror(code)` — human-readable description
- `pal_clear_error()` — resets thread state

## Memory Allocation

PAL Core provides the cross-boundary allocation functions:
- `pal_alloc(size)` — PAL-managed allocation
- `pal_free(ptr)` — PAL-managed deallocation
- `pal_realloc(ptr, new_size)` — PAL-managed reallocation
- `pal_secure_alloc(size)` — zeroized on free
- `pal_secure_free(ptr)` — secure deallocation

All memory returned by PAL functions is PAL-allocated. Kioto never frees PAL memory with free(); it passes PAL memory back through PAL interfaces.

## Backend Dispatch

The backend is registered as a `struct pal_ops` containing function pointers for every PAL operation. PAL Core validates handles and forwards operations to the backend.

The Host Adapter sets `pal_ops` during initialization. This is the single bridge between ABI and implementation.

## Key Rules

1. PAL Core never calls Host APIs directly — it only manages state and dispatches to the backend
2. Handles never leak internal details — only PAL Core knows the slot table layout
3. No backend logic in PAL Core — validation only, no translation
4. Thread safety: PAL Core uses mutexes only where documented by the ABI
5. No hidden allocations in PAL Core — all PAL memory flows through documented allocation functions

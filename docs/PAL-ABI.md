# PAL & ABI — How Platform Functions Work in Mire

This document explains the Platform Abstraction Layer (PAL), the ABI between
Mire code and native C code, and the step-by-step process for adding a new
platform function.

---

## Architecture Overview

```
Mire Source (.mire)
     │
     │  extern fn pal_foo: (host :str, port :i64) :i64 lib "c"
     │
     ▼
┌──────────────────────────────────────┐
│  Avenys Compiler (Rust → LLVM IR)    │
│                                      │
│  1. Parser sees `extern fn ... lib "c"` │
│  2. Generates `declare i64 @pal_foo(ptr, i64)` in LLVM IR │
│  3. clang links against pal_foo.o at link time │
└──────────────────────────────────────┘
     │
     │  call i64 @pal_foo(ptr %host, i64 %port)
     ▼
┌──────────────────────────────────────┐
│  PAL Backend (C)                     │
│                                      │
│  pal/                                  │
│    pal.h        ← all declarations     │
│    linux/        ← POSIX implementation│
│      pal_fs.c                          │
│      pal_net.c                         │
│      pal_tls.c                         │
│      pal_ws.c                          │
│    wasm/         ← WASM/WASI stubs     │
│      pal_fs.c                          │
│      pal_net.c                         │
└──────────────────────────────────────┘
     │
     ▼
  Operating System (Linux, WASM/WASI, ...)
```

---

## Type Mapping (ABI)

Mire's types map to C/LLVM types as follows:

| Mire Type   | C Type      | LLVM IR    | Notes |
|------------|-------------|------------|-------|
| `i64`      | `int64_t`   | `i64`      | 64-bit signed integer |
| `i32`      | `int32_t`   | `i32`      | 32-bit signed integer |
| `bool`     | `int`       | `i1`       | 0 or 1, widened to i64 for C |
| `f64`      | `double`    | `double`   | 64-bit float |
| `str`      | `const char *` | `ptr`   | Pointer to C string (null-terminated) |
| `void`     | `void`      | `void`     | No return value |
| `[T]`      | `void *`    | `ptr`      | Opaque pointer (lists, dicts) |

### Important Rules

1. **`str` → `const char *`**: Mire strings passed to C are null-terminated.
   The PAL receives ownership. Do not free the string in C — Mire's runtime
   manages the memory.

2. **`str` return → `char *`**: The PAL must return a `malloc`'d
   null-terminated string. The compiler automatically wraps it with
   `rt_managed_from_cstr()` and `free()`s the original — so the caller
   always receives a managed (ref-counted) string. Do NOT return
   managed strings or static buffers; always return freshly `malloc`'d
   (or `rt_strdup_raw`'d) memory.

3. **`fn -> void`**: PAL functions with no return value are declared as
   returning `void` in C. In LLVM IR they are declared as `void`.

4. **Pointers as `i64`**: Sockets, file descriptors, and other opaque handles
   are passed as `int64_t`. C casts them internally. Never expose raw C
   pointers to Mire code.

---

## Step-by-Step: Adding a New PAL Function

This example adds a hypothetical `pal_dns_lookup(host)` that resolves a
hostname to an IP address.

### Step 1: Declare in `pal.h`

```c
// src/pal/pal.h

// ── Networking ──────────────────────────────────────────────────
// ...existing declarations...
char *pal_dns_lookup(const char *host);
```

Rules:
- Use `const char *` for input strings.
- Return `char *` for strings (caller takes ownership).
- Return `int64_t` for handles and counts.
- Return `int` or `void` for side-effect-only operations.

### Step 2: Implement in `pal/linux/pal_net.c`

```c
// src/pal/linux/pal_net.c

#include "../pal.h"
#include <netdb.h>
#include <arpa/inet.h>
#include <stdlib.h>
#include <string.h>

char *pal_dns_lookup(const char *host) {
    struct hostent *he = gethostbyname(host);
    if (!he || !he->h_addr_list[0]) return NULL;

    struct in_addr addr;
    memcpy(&addr, he->h_addr_list[0], sizeof(addr));

    // Must return a malloc'd copy. Mire's runtime manages the memory.
    char *result = (char *)malloc(64);
    if (!result) return NULL;
    strcpy(result, inet_ntoa(addr));
    return result;
}
```

### Step 3: Add stub in `pal/wasm/pal_net.c`

```c
// src/pal/wasm/pal_net.c

#include "../pal.h"

char *pal_dns_lookup(const char *host) {
    (void)host;
    return NULL;  // Not supported in WASM
}
```

### Step 4: Register LLVM declaration in `llvm_functions.rs`

```rust
// src/avens/llvm_functions.rs

// Add near the other NET declarations:
"declare ptr @pal_dns_lookup(ptr)".to_string(),
```

The LLVM declaration follows this pattern:
- `declare` keyword
- Return type: `ptr` for `char *`, `i64` for `int64_t`, `i32` for `int`, `void` for void
- `@pal_<name>`
- Parameters: `ptr` for `const char *`, `i64` for `int64_t`, `i32` for `int`

### Step 5: Declare in Mire module (kioto)

```mire
# kioto/core/net/mod.mire

# Private implementation detail — not exported to other modules.
# Use `pub extern fn` if the symbol must be visible externally.
extern fn pal_dns_lookup: (host :str) :str lib "c"
```

Then wrap it in a public function:

```mire
pub fn dns_lookup: (host :&str) :str {
    return pal_dns_lookup(*host)
}
```

**Visibility:** `extern fn` without `pub` is private to the module (default).
Use `pub extern fn` to export the symbol. It is recommended to keep `extern fn`
private and wrap them in `pub fn` for safety and encapsulation.

### Step 6: Compile and test

```bash
cd mire
cargo build --release
cargo run -- build test.mire
./bin/debug/test
```

---

## PAL Module Reference

### Directory structure

```
src/pal/
  pal.h             ← All function declarations (umbrella header)
  linux/
    pal_fs.c         ← Filesystem (fopen/stat/mkdir)
    pal_env.c        ← Environment (getenv/setenv/chdir)
    pal_proc.c       ← Process (popen/fork/kill)
    pal_time.c       ← Time (clock_gettime)
    pal_cpu.c        ← CPU info (/proc/cpuinfo)
    pal_mem.c        ← Memory info (/proc/meminfo)
    pal_gpu.c        ← GPU info
    pal_term.c       ← Terminal styling (ANSI escapes)
    pal_net.c        ← TCP sockets + poll + DNS
    pal_tls.c        ← OpenSSL TLS client
    pal_ws.c         ← WebSocket (+ WSS TLS variant)

    pal_io.c         ← stdin/stdout/stderr
  wasm/
    pal_fs.c         ← WASI filesystem (fopen/stat via wasi-libc)
    pal_env.c        ← WASI environment
    pal_proc.c       ← Stubs
    pal_time.c       ← WASI clock
    pal_cpu.c        ← Stubs
    pal_mem.c        ← Stubs
    pal_gpu.c        ← Stubs
    pal_term.c       ← Stubs
    pal_net.c        ← Stubs
    pal_tls.c        ← Stubs
    pal_ws.c         ← Stubs

    pal_io.c         ← WASI I/O
```

### How the compiler selects a PAL backend

Avenys reads the `MIRE_PAL` environment variable at compile time:

```bash
# Linux (default)
MIRE_PAL=linux mire build main.mire

# WASM/WASI
MIRE_PAL=wasm mire build main.mire --target wasm32-wasi
```

The C files for the selected backend are compiled and linked into the final
binary. The `MIRE_PAL=wasm` target also sets clang's triple to
`wasm32-wasi`.

---

## Linkage

Non-standard C libraries (like OpenSSL) must declare extra link flags:

### In `src/avens/toolchain.rs`:

```rust
if !matches!(target, "...") {
    link_args.push("-lssl".to_string());
    link_args.push("-lcrypto".to_string());
}
```

### In `pal/linux/pal_tls.c`:

```c
#include <openssl/ssl.h>
#include <openssl/err.h>
// Use SSL functions...
```

The `pal.h` header keeps the public API clean (no OpenSSL types). The
implementation file includes the library headers internally.

---

## Runtime Core (`src/runtime/`)

The runtime core is platform-independent C code that provides:

| File | Purpose |
|------|---------|
| `managed.c` | Ref-counted strings (`rt_managed_*`) |
| `strings.c` | String ops (concat, split, substr, format, ...) |
| `lists.c` | List ops (create, push, pop, concat, slice) |
| `dicts.c` | Dict ops (get, set, keys, values) |

These are always linked regardless of platform. They use only standard C
(malloc, memcpy, sprintf) and no OS-specific APIs.

---

## Common Conventions

1. **Error returns**: Use -1 for `i64` failures, NULL for `str` failures,
   0 for `bool` failures. This matches POSIX conventions.

2. **Memory ownership**: The PAL returns owned memory (malloc'd). Mire's
   runtime calls `rt_managed_free()` or `free()` when done. The PAL must
   NOT free returned strings.

3. **Thread safety**: PAL functions may be called from any Mire goroutine
   (async task). Use locks if state is shared.

4. **Non-blocking I/O**: For networking, the PAL implements non-blocking
   connect with `poll()`. Mire's async runtime uses this to avoid blocking
   the event loop.

5. **WASM stubs**: Every PAL function must exist in `pal/wasm/`. Stubs
   either return error codes or trivially succeed. This ensures the code
   compiles for WASM targets even if the functionality isn't available.

---

## Troubleshooting

| Symptom | Likely Cause |
|---------|-------------|
| `undefined reference to pal_foo` at link | Missing `declare` in `llvm_functions.rs` OR missing C implementation |
| Segfault in PAL function | String ownership issue (PAL freed a Mire string) OR null pointer not checked |
| `str` argument is garbage | Parameter type mismatch — use `const char *` not `char *` |
| `str` return causes leak | Not returning `malloc`'d memory — use `strdup()` or `malloc()+strcpy()` |
| Compile fails on WASM target | Missing stub in `pal/wasm/` |

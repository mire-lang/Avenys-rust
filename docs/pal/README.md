# PAL v4 - Platform Abstraction Layer

> Capability-based host interaction with tests and examples.

Version: **4.1.0**
Status: **Stable**
Date: **2026-09-12**

---

## Overview

The **Platform Abstraction Layer (PAL) v4** provides a stable, capability-based interface for host system interaction. All PAL functions use **C ABI** (target platform calling convention) and are declared via `extern fn ... lib "c"`.

### Design Principles

- **Capability-based**: Operations require a `pal_root_t` handle (directory capability)
- **No symlink following**: Symlinks are never traversed; they are unlinked, not entered
- **Explicit error codes**: `pal_error_code_t` returned by all operations
- **Thread-safe**: Handle table uses generation-based validation
- **Sandboxed by default**: Absolute-path operations gated behind `PAL_ALLOW_UNSANDBOXED`

---

## Error Codes

```c
typedef enum {
    PAL_ERR_OK = 0,
    PAL_ERR_NOT_FOUND,      // ENOENT, ENOTDIR
    PAL_ERR_PERMISSION,     // EACCES, EPERM, ELOOP, EXDEV
    PAL_ERR_IO,             // Default for other errors
    PAL_ERR_INVALID,        // EISDIR, EINVAL, ENAMETOOLONG
    PAL_ERR_NO_MEM,         // ENOMEM
    PAL_ERR_BUSY,           // EBUSY
    PAL_ERR_UNSUPPORTED,    // ENOSYS, ENOTSUP
    PAL_ERR_ALREADY_EXISTS, // EEXIST
    PAL_ERR_INVALID_HANDLE, // EBADF
    PAL_ERR_OWNERSHIP,      // Ownership conflict
    PAL_ERR_NOT_EMPTY,      // ENOTEMPTY, EEXIST
} pal_error_code_t;
```

**Mapping**: `pal_core_errno_map(errno)` converts system errno to PAL error codes.

---

## Core PAL

### Handle Management

```c
typedef struct { uint32_t index; uint32_t generation; } pal_handle_t;

pal_handle_t pal_core_reserve(pal_type_t type);
void pal_core_release(pal_handle_t handle);
bool pal_core_validate(pal_handle_t handle, pal_type_t expected_type);
void *pal_core_get(pal_handle_t handle);
```

### Memory

```c
void *pal_alloc(size_t size);
void pal_free(void *ptr);
void *pal_realloc(void *ptr, size_t size);
```

---

## Filesystem PAL (`pal_fs.h`)

### Capability Primitives

```c
pal_root_t pal_root_open(const char *path);
void pal_root_close(pal_root_t root);

bool pal_root_remove(pal_root_t root, const char *rel_path);
pal_dir_t pal_dir_open(pal_root_t root, const char *rel_path);
bool pal_dir_next_name(pal_dir_t dir, char *out_buf, size_t cap);
void pal_dir_close(pal_dir_t dir);

pal_file_t pal_file_open(pal_root_t root, const char *rel_path, int flags);
size_t pal_file_read(pal_file_t file, void *buf, size_t len);
size_t pal_file_write(pal_file_t file, const void *buf, size_t len);
int64_t pal_file_seek(pal_file_t file, int64_t offset, int whence);
int64_t pal_file_size(pal_file_t file);
void pal_file_close(pal_file_t file);
```

### Host-Only Operations

```c
// Gated behind PAL_ALLOW_UNSANDBOXED (default 0)
bool pal_fs_exists(const char *path);
bool pal_fs_mkdir(const char *path);
bool pal_fs_rmdir(const char *path);
bool pal_fs_unlink(const char *path);
bool pal_fs_remove(const char *path);
char *pal_fs_read_file(const char *path);    // [PAL-OWNED]
```

### Symlink Policy

- **Symlinks are NEVER followed** during capability operations
- Trailing symlink is unlinked, not entered
- Intermediate symlinks in path -> `PAL_ERR_PERMISSION` (ELOOP)
- Symlinks pointing outside root are valid state

---

## Process PAL (`pal_proc.h`)

```c
typedef enum { PAL_SPAWN_WAIT, PAL_SPAWN_DETACH, PAL_SPAWN_PIPE } pal_spawn_mode_t;

pal_process_t pal_proc_create(const char *const *argv, pal_spawn_mode_t mode,
                               pal_channel_t stdin_ch, pal_channel_t stdout_ch, pal_channel_t stderr_ch);
int64_t pal_proc_wait(pal_process_t proc);
void pal_proc_kill(pal_process_t proc);
void pal_proc_close(pal_process_t proc);
```

### Channel Integration

```c
pal_channel_t pal_channel_create(void);
size_t pal_channel_send(pal_channel_t ch, const void *buf, size_t len);
pal_bytes_t pal_channel_recv(pal_channel_t ch);    // {ptr, len} = 16 bytes
void pal_channel_close(pal_channel_t ch);
```

Legacy shell (gated behind `PAL_ALLOW_LEGACY_SHELL`, default 0):
```c
int pal_proc_system(const char *cmd);
char *pal_proc_capture_output(const char *cmd);    // [PAL-OWNED]
```

---

## Crypto PAL (`pal_crypto.h`)

All functions return `pal_error_code_t` (0 = OK). Uses libsodium.

```c
pal_error_code_t pal_crypto_sha256(const uint8_t *input, size_t len, uint8_t *out32);
pal_error_code_t pal_crypto_sha512(const uint8_t *input, size_t len, uint8_t *out64);
pal_error_code_t pal_crypto_ed25519_keypair(uint8_t *out_pub32, uint8_t *out_priv64);
pal_error_code_t pal_crypto_ed25519_sign(const uint8_t *msg, size_t len, const uint8_t *priv64, uint8_t *out_sig64);
pal_error_code_t pal_crypto_ed25519_verify(const uint8_t *msg, size_t len, const uint8_t *pub32, const uint8_t *sig64);
pal_error_code_t pal_crypto_random_bytes(void *buf, size_t len);
```

---

## Network PAL (`pal_net.h`)

```c
pal_socket_t pal_socket_connect(const char *host, uint16_t port);
size_t pal_socket_send(pal_socket_t sock, const void *buf, size_t len);
size_t pal_socket_recv(pal_socket_t sock, void *buf, size_t len);
void pal_socket_close(pal_socket_t sock);

pal_listener_t pal_listener_bind(const char *host, uint16_t port);
pal_socket_t pal_listener_accept(pal_listener_t listener);
size_t pal_listener_send(pal_listener_t listener, const void *buf, size_t len);
size_t pal_listener_recv(pal_listener_t listener, void *buf, size_t len);
void pal_listener_close(pal_listener_t listener);
```

---

## Time PAL (`pal_time.h`)

```c
uint64_t pal_time_now_ms(void);
uint64_t pal_time_now_ns(void);
void pal_time_sleep_ms(uint64_t ms);
```

---

## Environment PAL (`pal_env.h`)

```c
const char *pal_env_get(const char *name);     // [BORROWED] (static)
const char *pal_env_cwd(void);                 // [BORROWED]
bool pal_env_set(const char *name, const char *value);
bool pal_env_unset(const char *name);
```

---

## Ownership Conventions

| Annotation | Meaning |
|------------|---------|
| `[PAL-OWNED]` | Caller must free with `pal_free()`; NULL on error |
| `[BORROWED]` | Static/thread-local; do NOT free |
| `[WRITE-INTO]` | Caller provides buffer; function writes into it |

Examples:
- `pal_fs_read_file()` -> `[PAL-OWNED]`
- `pal_proc_capture_output()` -> `[PAL-OWNED]`
- `pal_env_get()` -> `[BORROWED]`
- `pal_channel_recv()` -> `[WRITE-INTO]`

---

## Platform Implementations

| Platform | Directory | Status |
|----------|-----------|--------|
| Linux | `pal/linux/` | Complete |
| WASI Preview 1 | `pal/wasi/` | Complete |
| Darwin/macOS | `pal/darwin/` | Planned |
| Windows | `pal/windows/` | Planned |
| FreeBSD | `pal/freebsd/` | Planned |

---

## Compilation Flags

```c
#define PAL_ALLOW_UNSANDBOXED 1     // Enable absolute-path ops (default 1)
#define PAL_ALLOW_LEGACY_SHELL 0    // Enable shell functions (default 0)
```

---

## Tests

The PAL conformance tests validate every capability primitive:

```bash
cargo test --release --test pal_conformance
# Expected: 8/8 pass
# Tests cover: fs, proc, channel, crypto, net, time, env, handle safety
```

### Test Categories

1. **fs** - `pal_root_open`, `pal_root_remove`, `pal_dir_open`, `pal_dir_next_name`, `pal_file_open`, `pal_file_read`, `pal_file_write`
2. **proc** - `pal_proc_create`, `pal_proc_wait`, `pal_proc_kill`
3. **channel** - `pal_channel_create`, `pal_channel_send`, `pal_channel_recv`, `pal_channel_close`
4. **crypto** - `pal_crypto_sha256`, `pal_crypto_sha512`, `pal_crypto_ed25519_keypair`, `pal_crypto_ed25519_sign`, `pal_crypto_ed25519_verify`, `pal_crypto_random_bytes`
5. **net** - `pal_socket_connect`, `pal_listener_bind`, `pal_listener_accept`
6. **time** - `pal_time_now_ms`, `pal_time_now_ns`, `pal_time_sleep_ms`
7. **env** - `pal_env_get`, `pal_env_cwd`, `pal_env_set`, `pal_env_unset`
8. **handle safety** - validate/release/reuse of handle table slots

---

## Kioto Bindings

Kioto wraps PAL primitives in composition:

```mire
extern fn pal_root_remove: (root :i64, path :&str) :bool lib "c"
extern fn pal_last_error: () :i64 lib "c"

pub fn remove: (path :&str) :bool {
    set root = pal_root_open(fs::path::dir(path))
    if root <= 0 { return false }
    set ok = pal_root_remove(root, fs::path::name(path))
    pal_root_close(root)
    return ok
}
```

---

## Related

- [Mire ABI v4](../abi/README.md)
- [Runtime Configuration](../rt/README.md)
- [Avenys Documentation Index](../README.md)

---

## License

GNU General Public License v3.0

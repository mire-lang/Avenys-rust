# PAL ABI v4 — Contract Draft

This document defines the ABI contract between Mire and any Host. It derives from the design in PAL-DESIGN.md. Every function listed here is a primitive that cannot be correctly composed from other PAL primitives.

## Ownership Rules

- Every resource has exactly one owner unless explicitly cloned
- Resources are move-only unless explicitly cloned
- Cloner creates a second independent owner
- Transfer changes the sole owner unambiguously
- PAL-allocated memory belongs to PAL; Kioto never frees it directly
- When a function receives a handle, it takes no ownership unless documented

## Error Categories

### Pure Operations
Return values directly. No error infrastructure required.
- `pal_time_now_ms() → int64`
- `pal_time_now_ns() → int64`
- `pal_cpu_count() → int64`
- `pal_mem_total() → int64`
- `pal_mem_available() → int64`
- `pal_mem_process() → int64`

### Resource Operations
Return invalid handle or zero on failure. Thread-local error state is set. Query with `pal_last_error()`.

## Types

### Opaque Handles

```c
/* All handle types are opaque. Only the Host Adapter knows their layout. */
typedef struct pal_root pal_root_t;
typedef struct pal_file pal_file_t;
typedef struct pal_dir pal_dir_t;
typedef struct pal_socket pal_socket_t;
typedef struct pal_listener pal_listener_t;
typedef struct pal_channel pal_channel_t;
typedef struct pal_process pal_process_t;
typedef struct pal_secret pal_secret_t;
typedef struct pal_pubkey pal_pubkey_t;
typedef struct pal_random pal_random_t;
```

### Invalid Handles

Each handle type has an invalid sentinel value. A function returning an invalid handle means failure. The thread error state is set.

```c
#define PAL_INVALID_ROOT  ((pal_root_t){0})
#define PAL_INVALID_FILE  ((pal_file_t){0})
#define PAL_INVALID_DIR   ((pal_dir_t){0})
#define PAL_INVALID_SOCKET ((pal_socket_t){0})
#define PAL_INVALID_LISTENER ((pal_listener_t){0})
#define PAL_INVALID_CHANNEL ((pal_channel_t){0})
#define PAL_INVALID_PROCESS ((pal_process_t){0})
#define PAL_INVALID_SECRET  ((pal_secret_t){0})
#define PAL_INVALID_PUBKEY  ((pal_pubkey_t){0})
#define PAL_INVALID_RANDOM  ((pal_random_t){0})
```

Note: These are initializer macros only. The actual internal layout is defined only in the Host Adapter implementation. The ABI contract sees only opaque types and sentinel values.

### Handle Validity

```c
bool pal_handle_is_valid_pal_file(pal_file_t h);
bool pal_handle_is_valid_pal_dir(pal_dir_t h);
bool pal_handle_is_valid_pal_socket(pal_socket_t h);
bool pal_handle_is_valid_pal_listener(pal_listener_t h);
bool pal_handle_is_valid_pal_channel(pal_channel_t h);
bool pal_handle_is_valid_pal_process(pal_process_t h);
bool pal_handle_is_valid_pal_secret(pal_secret_t h);
bool pal_handle_is_valid_pal_pubkey(pal_pubkey_t h);
bool pal_handle_is_valid_pal_random(pal_random_t h);
```

All handle validity checks are implemented in PAL Core. The Host Adapter never validates handles internally.

### Error Codes

```c
typedef enum {
    PAL_ERR_OK = 0,
    PAL_ERR_NOT_FOUND,
    PAL_ERR_PERMISSION,
    PAL_ERR_IO,
    PAL_ERR_INVALID,
    PAL_ERR_NO_MEM,
    PAL_ERR_BUSY,
    PAL_ERR_UNSUPPORTED,
    PAL_ERR_ALREADY_EXISTS,
    PAL_ERR_INVALID_HANDLE,
    PAL_ERR_OWNERSHIP,
} pal_error_code_t;

pal_error_code_t pal_last_error(void);
const char *pal_strerror(pal_error_code_t code);
void pal_clear_error(void);
```

### Algorithm Identifier

```c
typedef uint64_t pal_crypto_algorithm_t;
```

Algorithm identifiers are opaque u64 values registered by the Host Adapter at compile time. PAL Core never interprets them. New algorithms are added by the Host Adapter without changing the PAL ABI.

### Flags Types

```c
typedef uint32_t pal_open_flags;
typedef uint32_t pal_socket_flags;
typedef uint32_t pal_spawn_flags;
```

### Buffer Result

For operations that return variable-length data (reads, crypto operations):

```c
typedef struct {
    void *data;
    int64_t len;
} pal_bytes_t;
```

`data` is PAL-allocated. Kioto must not free it directly; it passes it back to PAL through appropriate release functions or accepts ownership when documented.

## Resource Operations

### Filesystem (Root → File → Directory)

Root provides the sandbox boundary. All paths are relative to a Root.

Acquire:
```c
pal_root_t pal_root_open(const char *path);
```
Returns an invalid Root on failure (thread error set).

Release:
```c
void pal_root_close(pal_root_t root);
```

Use:
```c
pal_file_t pal_file_open(pal_root_t root, const char *rel_path, pal_open_flags flags);
pal_dir_t pal_dir_open(pal_root_t root, const char *rel_path);
```
Both return invalid handles on failure.

```c
int64_t pal_file_read(pal_file_t file, void *buf, int64_t capacity);
int64_t pal_file_write(pal_file_t file, const void *buf, int64_t length);
int64_t pal_file_seek(pal_file_t file, int64_t offset, int whence);
bool pal_file_stat(pal_file_t file, void *out_stat);
int64_t pal_file_size(pal_file_t file);
pal_file_t pal_file_clone(pal_file_t file);
```

Directory iteration — read-only, does not modify directory state:
```c
pal_dir_entry_t pal_dir_next(pal_dir_t dir);
```

Release:
```c
void pal_file_close(pal_file_t file);
void pal_dir_close(pal_dir_t dir);
```

### Process Lifecycle

Acquire:
```c
pal_process_t pal_proc_create(const char **argv, pal_spawn_flags flags,
                              pal_channel_t stdin_channel,
                              pal_channel_t stdout_channel,
                              pal_channel_t stderr_channel);
```
Returns invalid process on failure. stdin/stdout/stderr are channel handles for streaming I/O. Ownership of channels transfers to the process.

Use:
```c
int64_t pal_proc_wait(pal_process_t proc);
bool pal_proc_kill(pal_process_t proc);
pal_channel_t pal_proc_stdin(pal_process_t proc);
pal_channel_t pal_proc_stdout(pal_process_t proc);
pal_channel_t pal_proc_stderr(pal_process_t proc);
```

Transfer:
```c
pal_process_t pal_proc_transfer(pal_process_t proc);
```
Transfers ownership from current owner to caller. Previous owner loses all access.

Release:
```c
void pal_proc_close(pal_process_t proc);
```

### Networking

Acquire:
```c
pal_socket_t pal_socket_connect(const char *host, uint16_t port, pal_socket_flags flags);
pal_listener_t pal_listener_bind(uint16_t port, pal_socket_flags flags);
```

Use:
```c
pal_socket_t pal_listener_accept(pal_listener_t listener);
int64_t pal_socket_send(pal_socket_t sock, const void *buf, int64_t length);
int64_t pal_socket_recv(pal_socket_t sock, void *buf, int64_t capacity);
```

Release:
```c
void pal_socket_close(pal_socket_t sock);
void pal_listener_close(pal_listener_t listener);
```

### Channels

Acquire:
```c
pal_channel_t pal_channel_create(void);
```

Use:
```c
bool pal_channel_send(pal_channel_t ch, const void *buf, int64_t length);
pal_bytes_t pal_channel_recv(pal_channel_t ch);
```

Release:
```c
void pal_channel_close(pal_channel_t ch);
```

`pal_channel_recv` returns `pal_bytes_t{data, len}`. `data` is PAL-allocated. Kioto must copy the data before the next recv or close call, or accept ownership and free via PAL.

### Cryptography

Acquire:
```c
pal_secret_t pal_secret_create(pal_crypto_algorithm_t algorithm);
```
Returns invalid secret on failure (thread error set). Algorithm is validated against Host capability.

Use:
```c
pal_pubkey_t pal_secret_export_public(pal_secret_t secret);
int64_t pal_secret_sign(pal_secret_t secret, const void *msg, int64_t msg_len,
                         void *buf, int64_t capacity);
bool pal_pubkey_verify(pal_pubkey_t pubkey, const void *msg, int64_t msg_len,
                        const void *sig, int64_t sig_len);
```

Release:
```c
void pal_secret_close(pal_secret_t secret);
void pal_pubkey_free(pal_pubkey_t pubkey);
```

### Random

Stateless service:
```c
bool pal_random_fill(void *buf, int64_t length);
```
Returns false on failure (thread error set).

### Memory Allocation (PAL-owned)

PAL manages memory cross-boundary. Kioto never calls malloc/free directly when crossing ABI.

```c
void *pal_alloc(int64_t size);
void  pal_free(void *ptr);
void *pal_realloc(void *ptr, int64_t new_size);
void *pal_secure_alloc(int64_t size);
void  pal_secure_free(void *ptr);
```

`pal_alloc` and `pal_realloc` return NULL on allocation failure (sets thread error). Memory returned by PAL belongs to PAL. Kioto must use `pal_free` or `pal_secure_free` to release it.

### Attributes (Portable Macros)

```c
#define PAL_STABLE             /* committed for v4.x */
#define PAL_EXPERIMENTAL       /* may change without notice */
#define PAL_NODISCARD          __attribute__((warn_unused_result))
#define PAL_NONNULL(...)       __attribute__((nonnull(__VA_ARGS__)))
#define PAL_PURE               __attribute__((pure))
#define PAL_CONST              __attribute__((const))
#define PAL_MALLOC             __attribute__((malloc))
#define PAL_DEPRECATED(msg)    __attribute__((deprecated(msg)))
#define PAL_PRINTF(fmt, arg)   __attribute__((format(printf, fmt, arg)))
#define PAL_LIKELY(x)          __builtin_expect(!!(x), 1)
#define PAL_UNLIKELY(x)        __builtin_expect(!!(x), 0)
```

All attributes resolve to empty on compilers that do not support them.

## ABI Compatibility Rules

The following invariants are frozen for ABI v4:

- **close() always invalidates the resource handle** — after close, the handle must be detected as invalid
- **clone() always creates a new independent owner** — clone returns a fresh handle to the same resource
- **transfer() always changes the sole owner unambiguously** — after transfer, the old owner has no access
- **Root capabilities never expand authority** — opening a directory does not grant file access; each capability is scoped
- **read() never modifies ownership** — reading does not transfer or clone any handle
- **handle types never shrink in size** — handle structs may grow (reserved padding) but never shrink
- **enum values are never removed or reordered** — only additions allowed

Compatible changes (ABI-preserving):
- Adding new operations
- Adding new resource types
- Adding new capability types
- Adding new flags (with reserved bits)
- Adding new algorithm identifiers
- Expanding struct fields (with reserved padding)
- Adding new stateless service queries

Incompatible changes (require ABI renegotiation):
- Removing or renaming operations
- Changing ownership semantics
- Changing handle sizes or layouts
- Changing the meaning of close()
- Changing error code semantics
- Modifying resource protocol guarantees

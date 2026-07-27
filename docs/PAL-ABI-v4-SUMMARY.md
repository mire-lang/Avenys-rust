# PAL ABI v4 — Summary

## Architecture

```
Kioto (Mire standard library)
    ↓ extern fn calls
PAL ABI (pal.h) — stable contract
    ↓ dispatch
PAL Core (src/pal/core/) — handles, errors, allocators, validation
    ↓ backend ops
Host Adapter (src/pal/linux/) — syscalls only
```

## Key Design Decisions

### 1. Opaque Handles

Handles are `{uint32_t index; uint32_t generation}`. The ABI exposes only the typedef. No code outside PAL Core accesses internal data.

```c
typedef struct pal_handle {
    uint32_t index;
    uint32_t generation;
} pal_handle_t;

typedef pal_handle_t pal_file_t;
typedef pal_handle_t pal_root_t;
// ... etc
```

### 2. NULL Constants

```c
#define PAL_FILE_NULL ((pal_file_t){0, 0})
#define PAL_ROOT_NULL ((pal_root_t){0, 0})
// ... etc
```

### 3. Type-Safe Enums

```c
typedef enum {
    PAL_SEEK_BEGIN = 0,
    PAL_SEEK_CURRENT = 1,
    PAL_SEEK_END = 2,
} pal_seek_from_t;
```

Not POSIX `whence`. Linux translates to `SEEK_SET` etc.

### 4. PAL-owned Types

```c
typedef struct {
    uint64_t size;
    uint64_t mode;
    int64_t mtime_ns;
    int64_t ctime_ns;
    uint64_t dev;
    uint64_t ino;
} pal_stat_t;
```

Not `void*`. Not `struct stat`.

### 5. Root as Capability

Root stores an `fd`, not a path. All file operations use `openat()`:

```c
pal_file_t pal_file_open(pal_root_t root, const char *rel_path, pal_open_flags flags);
// Linux: openat(root->fd, rel_path, ...)
```

No string concatenation. No TOCTOU.

### 6. Stateless Services

Time, CPU, Memory, Random are direct queries. No handles:

```c
int64_t pal_time_now_ms(void);
int64_t pal_cpu_count(void);
int64_t pal_mem_total(void);
bool pal_random_fill(void *buf, int64_t length);
```

### 7. PAL-owned Memory

```c
void *pal_alloc(int64_t size);
void pal_free(void *ptr);
```

No `malloc`/`free`/`strdup` in Host Adapter.

## Function List (50 symbols)

### Filesystem (13)
- `pal_root_open`, `pal_root_close`
- `pal_file_open`, `pal_file_read`, `pal_file_write`, `pal_file_seek`, `pal_file_stat`, `pal_file_size`, `pal_file_clone`, `pal_file_close`
- `pal_dir_open`, `pal_dir_next`, `pal_dir_close`

### Process (8)
- `pal_proc_create`, `pal_proc_wait`, `pal_proc_kill`
- `pal_proc_stdin`, `pal_proc_stdout`, `pal_proc_stderr`
- `pal_proc_transfer`, `pal_proc_close`

### Channels (4)
- `pal_channel_create`, `pal_channel_send`, `pal_channel_recv`, `pal_channel_close`

### Networking (7)
- `pal_socket_connect`, `pal_listener_bind`, `pal_listener_accept`
- `pal_socket_send`, `pal_socket_recv`, `pal_socket_close`, `pal_listener_close`

### Crypto (6)
- `pal_secret_create`, `pal_secret_export_public`, `pal_secret_sign`
- `pal_pubkey_verify`, `pal_secret_close`, `pal_pubkey_free`

### Stateless Services (7)
- `pal_time_now_ms`, `pal_time_now_ns`, `pal_cpu_count`
- `pal_mem_total`, `pal_mem_available`, `pal_mem_process`
- `pal_random_fill`

### Memory (5)
- `pal_alloc`, `pal_free`, `pal_realloc`
- `pal_secure_alloc`, `pal_secure_free`

## Error Handling

Thread-local error state. Operations that fail return `PAL_*_NULL` or `-1`.

```c
pal_error_code_t pal_last_error(void);
const char *pal_strerror(pal_error_code_t code);
void pal_clear_error(void);
```

## What's NOT in PAL (moved to Kioto)

- `fs.copy`, `fs.move`, `fs.walk` — composition
- `fs.join`, `fs.dir`, `fs.name`, `fs.ext` — string operations
- `proc.run`, `proc.exec`, `proc.shell` — shell-dependent
- `env.get`, `env.set`, `env.cwd` — not fundamental primitives
- `thread.detach`, `thread.join` — not in v4 scope

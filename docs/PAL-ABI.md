# PAL ABI v4

This is the current Avenys PAL contract. The canonical declarations are in
[`src/pal/pal.h`](../src/pal/pal.h); this document explains their ownership,
lifecycle, and boundary rules. `docs/abi_map.toml` is the symbol catalogue
used by the ABI tests.

## Layers

```text
Kioto
  ↓ extern fn calls
PAL ABI (`src/pal/pal.h`)
  ↓ validated dispatch
PAL Core (`src/pal/core/`)
  ↓ host operations
Linux host adapter (`src/pal/linux/`)
```

The PAL contains host primitives. Composition such as copying a file, walking
a tree, decoding UTF-8, or launching a shell command belongs in Kioto.

## Handles and ownership

Every stateful handle is the same ABI-sized value:

```c
typedef struct pal_handle {
    uint32_t index;
    uint32_t generation;
} pal_handle_t;
```

The public aliases are `pal_root_t`, `pal_file_t`, `pal_dir_t`,
`pal_process_t`, `pal_socket_t`, `pal_listener_t`, `pal_channel_t`,
`pal_secret_t`, and `pal_pubkey_t`. A zero `{index, generation}` value is the
corresponding `PAL_*_NULL` invalid handle.

PAL Core validates the index, generation, resource type, and owner thread.
The host adapter's file descriptors, pointers, and OS-specific structures are
never part of the public ABI.

Stateful resources follow `acquire → use → release`. `clone` creates an
independent resource where the operation exists; `transfer` changes ownership
explicitly. Kioto must close every acquired handle exactly once.

## Pointer ownership conventions

Every PAL function that returns or receives a pointer documents its ownership:

- **`[PAL-OWNED]`** — the caller owns the returned memory and MUST release it
  with the documented function (usually `pal_free`). On failure these
  functions return `NULL`, never a string literal. Marked as such:
  `pal_fs_read_file`, `pal_proc_capture_output`, `pal_channel_recv`'s
  `pal_bytes_t.data`.
- **`[BORROWED]`** — the pointer is static or borrowed from the host; the
  caller MUST NOT free it, and it may be invalidated by a later PAL call.
  Marked as such: `pal_env_cwd`, `pal_env_get`.
- **`[WRITE-INTO]`** — the caller supplies the buffer; the PAL writes into it
  and returns a length/count. E.g. `pal_file_read`, `pal_dir_next_name`,
  `pal_proc_capture`.

New PAL functions should encode ownership in the symbol name where the ABI
can change (`*_owned`, `*_borrowed`); stable v4 symbols document it in the
header.

## Sandbox boundaries

The PAL is a capability model: files are reached **only** through a
`pal_root_t` handle with a relative path (`pal_file_open`, `pal_dir_open`).
Two legacy groups opt out of that model and are **compile-time gated** in
`pal.h` so they are never part of a hardened build:

- `PAL_ALLOW_UNSANDBOXED` — `pal_fs_*` absolute-path operations
  (`pal_fs_exists/mkdir/rmdir/unlink/read_file/remove`). They bypass root
  capabilities entirely and exist only for the runtime's own internal use.
- `PAL_ALLOW_LEGACY_SHELL` — `pal_proc_system`, `pal_proc_capture`,
  `pal_proc_capture_output`. They invoke `/bin/sh -c` and are a
  command-injection surface. Use `pal_proc_create` (argv-safe, no shell)
  instead.

Both toggles default to `1` in `pal.h` for runtime compatibility; flip to `0`
to strip them. Neither group is ever exposed to untrusted Mire code, and the
ABI map does not catalogue them.

## Struct-return (sret) ABI rule

The codegen has **no struct-return (sret) support**: PAL functions that return
a struct by value cannot be called from Mire. They stay in the C header for
host-side use but must be bridged by the runtime for Mire:

- `pal_dir_entry_t pal_dir_next(pal_dir_t)` (259 B, sret) → use
  `pal_dir_next_into` / `pal_dir_next_name`.
- `pal_bytes_t pal_channel_recv(pal_channel_t)` (16 B, returned in RAX:RDX) →
  use `rt_channel_recv_into`.

8-byte handles (`{u32,u32}`) are classified by the codegen as a single `i64`
and are safe to return by value.

## Errors

Operations return their documented result (`-1`, `false`, a null handle, or a
null/empty result) and set the thread-local PAL error where applicable:

```c
typedef enum {
    PAL_ERR_OK,
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
    PAL_ERR_NOT_EMPTY,
} pal_error_code_t;

pal_error_code_t pal_last_error(void);
const char *pal_strerror(pal_error_code_t code);
void pal_clear_error(void);
```

Errors must be checked before a caller discards a failure result. The PAL does
not retry, normalize paths, invoke a shell, convert encodings, or silently
replace unsupported host capabilities.

### errno mapping

Backend operations set `errno` on failure; the dispatch layer maps it to the
PAL error via `pal_core_errno_map(errno)` (in `src/pal/core/pal_core.c`):

| errno | PAL error |
|-------|-----------|
| `ENOENT`, `ENOTDIR` | `PAL_ERR_NOT_FOUND` |
| `EACCES`, `EPERM`, `ELOOP`, `EXDEV` | `PAL_ERR_PERMISSION` |
| `ENOTEMPTY`, `EEXIST` | `PAL_ERR_NOT_EMPTY` |
| `EISDIR`, `EINVAL`, `ENAMETOOLONG` | `PAL_ERR_INVALID` |
| `EBUSY` | `PAL_ERR_BUSY` |
| `ENOMEM` | `PAL_ERR_NO_MEM` |
| anything else | `PAL_ERR_IO` |

The open/create-family dispatch functions (`pal_root_open`, `pal_file_open`,
`pal_dir_open`, `pal_proc_create`, `pal_socket_connect`, `pal_listener_bind`,
`pal_listener_accept`, `pal_channel_create`, `pal_secret_create`) all use this
map, so a failed `pal_root_open` on a missing path reports `NOT_FOUND`, a
failed `pal_root_remove` on a non-empty directory reports `NOT_EMPTY`, etc.
Never report a hard-coded `PAL_ERR_IO` for a syscall failure — it hides the
real cause.

### Worked example: capability removal

```c
// Remove the empty directory "logs" under the root handle, then a file.
pal_root_t root = pal_root_open("/srv/app");      // cap = /srv/app
if (root.index == 0) { /* pal_last_error() == PAL_ERR_NOT_FOUND */ }

// unlinkat-based, relative to the root's fd; no path string is ever built.
bool ok1 = pal_root_remove(root, "logs");          // true
bool ok2 = pal_root_remove(root, "state/pid");     // true (nested parent ok)

bool ok3 = pal_root_remove(root, "data");          // false if "data" is
                                                   // non-empty:
// pal_last_error() == PAL_ERR_NOT_EMPTY; "data" was NOT touched.

// A symlink is unlinked, never followed:
bool ok4 = pal_root_remove(root, "link_to_etc");   // removes the link only;
                                                   // its target is untouched.

pal_root_close(root);
```

`..` is impossible: basenames of `.`, `..`, or empty are rejected with
`PAL_ERR_INVALID`, and the parent is resolved with `RESOLVE_NO_SYMLINKS`
(beneath-style mount crossings are allowed — `RESOLVE_BENEATH` would reject
ordinary paths that cross into tmpfs, e.g. relative-to-`/` access to `/tmp`).

## Primitive groups

The public header currently exposes:

- PAL-owned allocation: `pal_alloc`, `pal_free`, `pal_realloc`,
  `pal_secure_alloc`, `pal_secure_free`.
- Root-scoped filesystem: root/file/directory handles, byte read/write/seek,
  stat, size, clone, and directory entry iteration.
- Single-entry removal: `pal_root_remove(root, rel_path)` unlinks exactly one
  entry (regular file, symlink, or empty directory) relative to a root handle.
  A trailing symlink is unlinked, **never followed**; intermediate symlinks in
  the path are rejected. Non-empty directories fail with `PAL_ERR_NOT_EMPTY`.
  Recursive removal is a Kioto-side composition over `pal_dir_open` +
  `pal_root_remove` — it is not a PAL primitive (composition belongs in Kioto).
- Host process resources: create, wait, kill, standard-channel access,
  transfer, and close.
- Sockets, listeners, and byte channels.
- Cryptographic secret/public-key handles using the typed
  `pal_crypto_algorithm_t` constants.
- Threads and stateless host queries for time, CPU, memory, and randomness.
- Environment access (`pal_env_cwd`, `pal_env_get`): `[BORROWED]` static
  buffers — read-only, never freed.
- Absolute-path primitives (`pal_fs_*`, including `pal_fs_remove`) and shell
  helpers (`pal_proc_system`/`capture*`): compile-time gated behind
  `PAL_ALLOW_UNSANDBOXED` / `PAL_ALLOW_LEGACY_SHELL`; they do not implement
  filesystem composition and are never exposed to untrusted Mire code.

Use the declarations in `src/pal/pal.h` and the current `docs/abi_map.toml`
when adding a symbol. Do not copy signatures from historical documents.

## Typed values

Flags and origins are named ABI types, not strings:

```c
typedef uint32_t pal_open_flags;
typedef uint32_t pal_socket_flags;
typedef uint32_t pal_spawn_flags;
typedef enum {
    PAL_SEEK_BEGIN,
    PAL_SEEK_CURRENT,
    PAL_SEEK_END,
} pal_seek_from_t;
```

Variable byte results use `pal_bytes_t { void *data; int64_t len; }`. Ownership
of returned buffers is defined by each operation; callers must use the
matching PAL release operation rather than guessing an allocator.

## Adding or changing a symbol

1. Decide whether the operation is a Host primitive; if Kioto can compose it,
   do not add it to PAL.
2. Add the typed declaration to `src/pal/pal.h`.
3. Add the backend operation and dispatch implementation, preserving handle
   validation and error information.
4. If the codegen emits the symbol (a `declare` in
   `src/compiler/mir/codegen/builtins.rs`), register it in `docs/abi_map.toml`
   and update ABI conformance tests. Symbols Kioto declares directly as
   `extern fn ... lib "c"` (e.g. `pal_root_remove`, `pal_fs_remove`,
   `pal_last_error`) are NOT catalogued — adding them to the map creates a
   stale entry and fails `tests/abi_consistency.rs`.
5. Add a Kioto wrapper only when it contributes composition or Mire-facing
   semantics.

The ABI map and PAL conformance tests are authoritative for the compiler-
emitted host surface; `src/pal/pal.h` is authoritative for the full ABI.

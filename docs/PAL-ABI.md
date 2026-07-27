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
} pal_error_code_t;

pal_error_code_t pal_last_error(void);
const char *pal_strerror(pal_error_code_t code);
void pal_clear_error(void);
```

Errors must be checked before a caller discards a failure result. The PAL does
not retry, normalize paths, invoke a shell, convert encodings, or silently
replace unsupported host capabilities.

## Primitive groups

The public header currently exposes:

- PAL-owned allocation: `pal_alloc`, `pal_free`, `pal_realloc`,
  `pal_secure_alloc`, `pal_secure_free`.
- Root-scoped filesystem: root/file/directory handles, byte read/write/seek,
  stat, size, clone, and directory entry iteration.
- Host process resources: create, wait, kill, standard-channel access,
  transfer, and close.
- Sockets, listeners, and byte channels.
- Cryptographic secret/public-key handles using the typed
  `pal_crypto_algorithm_t` constants.
- Threads and stateless host queries for time, CPU, memory, and randomness.
- Explicit environment and absolute-path primitives currently present in
  `pal.h`; these remain low-level ABI calls and do not implement filesystem
  composition.

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
4. Register the symbol in `docs/abi_map.toml` and update ABI conformance tests.
5. Add a Kioto wrapper only when it contributes composition or Mire-facing
   semantics.

The ABI map and PAL conformance tests are authoritative for the currently
implemented host surface.

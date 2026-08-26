#ifndef MIRE_PAL_H
#define MIRE_PAL_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

// PAL v4 — Host Resource Model
// ABI contract between Mire and any Host.
// Handles are opaque: {index, generation}. Only PAL Core knows the slot table.
// Host Adapters receive internal data through Core dispatch, not through handle fields.

// ── Build Toggles ─────────────────────────────────────────────
// Unsandboxed absolute-path FS ops remain on for runtime compat (opt-out via
// `-DPAL_ALLOW_UNSANDBOXED=0`). The legacy shell (pal_proc_system /
// pal_proc_capture* / rt_proc_capture_output → /bin/sh -c) is OFF by default:
// no runtime surface invokes a shell anymore (kioto proc is fully argv-based).
// Re-enable with `-DPAL_ALLOW_LEGACY_SHELL=1` only for explicitly-trusted code.
#ifndef PAL_ALLOW_UNSANDBOXED
#define PAL_ALLOW_UNSANDBOXED 1
#endif
#ifndef PAL_ALLOW_LEGACY_SHELL
#define PAL_ALLOW_LEGACY_SHELL 0
#endif

// ── Opaque Handle Types ──────────────────────────────────────
// Each handle is {uint32_t index; uint32_t generation}.
// The ABI exposes the struct definition so it has a known size (8 bytes).
// Only PAL Core and Host Adapter access the fields.

typedef struct pal_handle {
    uint32_t index;
    uint32_t generation;
} pal_handle_t;

typedef pal_handle_t pal_root_t;
typedef pal_handle_t pal_file_t;
typedef pal_handle_t pal_dir_t;
typedef pal_handle_t pal_socket_t;
typedef pal_handle_t pal_listener_t;
typedef pal_handle_t pal_channel_t;
typedef pal_handle_t pal_process_t;
typedef pal_handle_t pal_secret_t;
typedef pal_handle_t pal_pubkey_t;

// NULL constants (all handles zero-initialized are invalid)
#define PAL_ROOT_NULL     ((pal_root_t){0, 0})
#define PAL_FILE_NULL     ((pal_file_t){0, 0})
#define PAL_DIR_NULL      ((pal_dir_t){0, 0})
#define PAL_SOCKET_NULL   ((pal_socket_t){0, 0})
#define PAL_LISTENER_NULL ((pal_listener_t){0, 0})
#define PAL_CHANNEL_NULL  ((pal_channel_t){0, 0})
#define PAL_PROCESS_NULL  ((pal_process_t){0, 0})
#define PAL_SECRET_NULL   ((pal_secret_t){0, 0})
#define PAL_PUBKEY_NULL   ((pal_pubkey_t){0, 0})

// ── Error Codes ──────────────────────────────────────────────

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
    PAL_ERR_NOT_EMPTY,
} pal_error_code_t;

// ── Flags (type-safe, not raw integers) ─────────────────────

typedef uint32_t pal_open_flags;
#define PAL_OPEN_READ    0x0001
#define PAL_OPEN_WRITE   0x0002
#define PAL_OPEN_CREATE  0x0004
#define PAL_OPEN_TRUNCATE 0x0008
#define PAL_OPEN_APPEND  0x0010

typedef uint32_t pal_socket_flags;
#define PAL_SOCKET_TCP 0x0001
#define PAL_SOCKET_UDP 0x0002

typedef uint32_t pal_spawn_flags;
#define PAL_SPAWN_WAIT 0x0001

// ── Seek Origin (not POSIX whence) ──────────────────────────

typedef enum {
    PAL_SEEK_BEGIN   = 0,
    PAL_SEEK_CURRENT = 1,
    PAL_SEEK_END     = 2,
} pal_seek_from_t;

// ── PAL-owned Stat ──────────────────────────────────────────

typedef struct {
    uint64_t size;
    uint64_t mode;
    int64_t  mtime_ns;
    int64_t  ctime_ns;
    uint64_t dev;
    uint64_t ino;
} pal_stat_t;

// ── PAL-owned Bytes ─────────────────────────────────────────

// 16-byte value struct (data ptr + len). Returned by value on x86-64 SysV
// in RAX:RDX — safe for FFI. [data is PAL-OWNED when returned from a call;
// release with pal_free(data).]
typedef struct {
    void *data;
    int64_t len;
} pal_bytes_t;

// ── Directory Entry (PAL-owned) ─────────────────────────────

// 259-byte value struct: returned by value on x86-64 SysV via a hidden
// sret pointer — do NOT call pal_dir_next from FFI; use pal_dir_next_into
// or pal_dir_next_name instead.
typedef struct {
    char name[256];
    bool is_file;
    bool is_dir;
    bool is_symlink;
} pal_dir_entry_t;

// ── Crypto Algorithm Registry ───────────────────────────────

typedef uint64_t pal_crypto_algorithm_t;
#define PAL_CRYPTO_ED25519 1
#define PAL_CRYPTO_X25519  2

// ── Error System ─────────────────────────────────────────────
pal_error_code_t pal_last_error(void);
const char *pal_strerror(pal_error_code_t code);
void pal_clear_error(void);
// [BORROWED] Message from the last pal_set_error on this thread; NULL if none.
const char *pal_last_error_message(void);

// ── Memory Ownership Conventions ─────────────────────────────
// Every pointer-returning PAL function documents its ownership:
//   * [PAL-OWNED]  → the caller owns the memory and MUST release it with
//                    the documented function (usually `pal_free`). It is
//                    never NULL-with-""; failure returns NULL.
//   * [BORROWED]   → the pointer is static/borrowed. The caller MUST NOT
//                    free it. It may be invalidated by a later PAL call.
//   * [WRITE-INTO] → caller-provided buffer; the PAL writes into it.
// New PAL functions should encode ownership in the name where the ABI can
// change (`*_owned`, `*_borrowed`); stable ABI functions document it here.

// ── Memory (PAL-owned) ─────────────────────────────────────
void *pal_alloc(int64_t size);
void pal_free(void *ptr);
void *pal_realloc(void *ptr, int64_t new_size);
void *pal_secure_alloc(int64_t size);
void pal_secure_free(void *ptr);

// ── Filesystem ──────────────────────────────────────────────
pal_root_t pal_root_open(const char *path);
void pal_root_close(pal_root_t root);
pal_file_t pal_file_open(pal_root_t root, const char *rel_path, pal_open_flags flags);
int64_t pal_file_read(pal_file_t file, void *buf, int64_t capacity);
int64_t pal_file_write(pal_file_t file, const void *buf, int64_t length);
int64_t pal_file_seek(pal_file_t file, int64_t offset, pal_seek_from_t from);
bool pal_file_stat(pal_file_t file, pal_stat_t *out);
int64_t pal_file_size(pal_file_t file);
pal_file_t pal_file_clone(pal_file_t file);
void pal_file_close(pal_file_t file);
pal_dir_t pal_dir_open(pal_root_t root, const char *rel_path);
/// Returns the next directory entry by value (struct return ABI). Use
/// pal_dir_next_into or pal_dir_next_name for safe FFI across language boundaries.
pal_dir_entry_t pal_dir_next(pal_dir_t dir);
/// Safe FFI variant of pal_dir_next: writes entry into caller-allocated out buffer.
/// Avoids the hidden-pointer struct-return ABI issue on x86-64 SysV.
void pal_dir_next_into(pal_dir_t dir, pal_dir_entry_t *out);
/// Kioto-friendly variant: writes only the entry name into out[0..cap-1], null-terminated.
/// Returns name length (>0 if found, 0 if no more entries, -1 on error).
int64_t pal_dir_next_name(pal_dir_t dir, char *out, int64_t cap);
void pal_dir_close(pal_dir_t dir);
/// Capability-based removal of a SINGLE entry relative to a root handle.
/// Removes a file, symlink, or empty directory. A symlink is unlinked — never
/// followed — so a link pointing outside the root is normal state and is not
/// rejected. Parent directories are resolved with RESOLVE_NO_SYMLINKS on
/// Linux (openat2): intermediate components that are symlinks are rejected
/// with PAL_ERR_PERMISSION, and "."/".."/empty basenames are invalid. Recursive
/// removal is NOT a PAL operation; compose it in the host (kioto fs::remove_all)
/// over pal_dir_open + this primitive.
/// Returns false and sets pal_last_error() on failure:
///   PAL_ERR_NOT_FOUND, PAL_ERR_PERMISSION, PAL_ERR_NOT_EMPTY (dir not empty),
///   PAL_ERR_INVALID (empty/`.`/`..`/trailing-slash path), PAL_ERR_IO.
bool pal_root_remove(pal_root_t root, const char *rel_path);

// ══ UNSANDBOXED ── absolute-path filesystem operations ────────
// WARNING: these take raw absolute paths and bypass the root-capability
// model entirely. They are included for the OWN runtime/Owl internal use
// only and must NEVER be exposed to untrusted Mire code. Prefer the
// root-relative API above (pal_file_open / pal_dir_open on a pal_root_t).
#if PAL_ALLOW_UNSANDBOXED
bool pal_fs_exists(const char *path);
bool pal_fs_mkdir(const char *path);
bool pal_fs_rmdir(const char *path);
bool pal_fs_unlink(const char *path);
/// Remove a SINGLE entry by absolute path (file, symlink, or empty directory).
/// Like POSIX remove(3): a symlink is unlinked, never followed. Recursive
/// removal is NOT a PAL operation; compose it host-side. Returns false and
/// sets pal_last_error() (PAL_ERR_NOT_FOUND / PAL_ERR_PERMISSION /
/// PAL_ERR_NOT_EMPTY / PAL_ERR_INVALID / PAL_ERR_IO).
bool pal_fs_remove(const char *path);
#endif // PAL_ALLOW_UNSANDBOXED
// pal_fs_read_file: [PAL-OWNED] returns a malloc'd, NUL-terminated string.
// Caller MUST release with pal_free(). Returns NULL on error (never a
// literal). Retained under PAL_ALLOW_UNSANDBOXED for the runtime only;
// kioto prefers rt_fs_read_bytes (runtime-managed copy).
#if PAL_ALLOW_UNSANDBOXED
const char *pal_fs_read_file(const char *path);
#endif // PAL_ALLOW_UNSANDBOXED

// ── Filesystem Path Utilities ──────────────────────────
// These operate on absolute paths (UNSANBOXED). They are included
// for runtime/Owl internal use only and must NEVER be exposed to
// untrusted Mire code. Prefer the root-relative API above.
#if PAL_ALLOW_UNSANDBOXED
const char *pal_fs_ext(const char *path);
const char *pal_fs_dir(const char *path);
const char *pal_fs_name(const char *path);
bool pal_fs_is_file(const char *path);
bool pal_fs_copy(const char *src, const char *dst);
bool pal_fs_move(const char *src, const char *dst);
#endif // PAL_ALLOW_UNSANDBOXED

// ── Environment ──────────────────────────────────────────
// pal_env_cwd / pal_env_get: [BORROWED] static/process-owned buffer.
// Caller MUST NOT free. May be invalidated by a later PAL call.
const char *pal_env_cwd(void);
const char *pal_env_get(const char *name);
// pal_env_all: [PAL-OWNED] returns a map[str str] of all environment
// variables. Caller MUST release with pal_free().
const char *pal_env_all(void);

// ══ LEGACY SHELL ── only compiled when PAL_ALLOW_LEGACY_SHELL is set ──
// pal_proc_system / pal_proc_capture / pal_proc_capture_output run a
// shell (`/bin/sh -c`) and are a command-injection surface. They exist for
// compatibility only. NEVER expose to untrusted Mire code; use
// pal_proc_create (argv-safe, no shell) instead.
// pal_proc_capture_output: [PAL-OWNED] malloc'd output; caller MUST free.
#if PAL_ALLOW_LEGACY_SHELL
int64_t pal_proc_system(const char *cmd);
int64_t pal_proc_capture(const char *cmd, void *buf, int64_t capacity);
const char *pal_proc_capture_output(const char *cmd);
#endif // PAL_ALLOW_LEGACY_SHELL
// pal_proc_wait_pid: PID-based waitpid for rare cases where you only
// have a raw OS PID (e.g. from pal_proc_spawn's fork). Prefer
// pal_proc_wait(pal_process_t) which is handle-based, validated, and safe.
int64_t pal_proc_wait_pid(int64_t pid);

// ── Process ──────────────────────────────────────────────────
pal_process_t pal_proc_create(const char **argv, pal_spawn_flags flags,
                            pal_channel_t stdin_ch, pal_channel_t stdout_ch,
                            pal_channel_t stderr_ch);
int64_t pal_proc_wait(pal_process_t proc);
bool pal_proc_kill(pal_process_t proc);
pal_channel_t pal_proc_stdin(pal_process_t proc);
pal_channel_t pal_proc_stdout(pal_process_t proc);
pal_channel_t pal_proc_stderr(pal_process_t proc);
pal_process_t pal_proc_transfer(pal_process_t proc);
void pal_proc_close(pal_process_t proc);
// pal_proc_exists: check if a process with the given PID is still running.
// Returns true if the process exists, false otherwise.
bool pal_proc_exists(int64_t pid);
// pal_proc_run: run a command via argv (no shell). Returns the exit code.
// This is a convenience wrapper around pal_proc_create + pal_proc_wait.
int64_t pal_proc_run(const char *cmd, const char **argv);

// ── I/O ────────────────────────────────────────────────────
// pal_io_print_err: write a message to stderr. [BORROWED] the message
// is not copied; the caller must ensure the string remains valid.
void pal_io_print_err(const char *msg);

// ── Networking ─────────────────────────────────────────────
pal_socket_t pal_socket_connect(const char *host, uint16_t port, pal_socket_flags flags);
pal_listener_t pal_listener_bind(uint16_t port, pal_socket_flags flags);
pal_socket_t pal_listener_accept(pal_listener_t listener);
int64_t pal_socket_send(pal_socket_t sock, const void *buf, int64_t length);
int64_t pal_socket_recv(pal_socket_t sock, void *buf, int64_t capacity);
void pal_socket_close(pal_socket_t sock);
void pal_listener_close(pal_listener_t listener);

// ── Channels ─────────────────────────────────────────────────
pal_channel_t pal_channel_create(void);
int64_t pal_channel_send(pal_channel_t ch, const void *buf, int64_t length);
pal_bytes_t pal_channel_recv(pal_channel_t ch);
void pal_channel_close(pal_channel_t ch);

// ── Crypto ─────────────────────────────────────────────────
pal_secret_t pal_secret_create(pal_crypto_algorithm_t algorithm);
pal_pubkey_t pal_secret_export_public(pal_secret_t secret);
int64_t pal_secret_sign(pal_secret_t secret, const void *msg, int64_t msg_len,
                        void *buf, int64_t capacity);
bool pal_pubkey_verify(pal_pubkey_t pubkey, const void *msg, int64_t msg_len,
                       const void *sig, int64_t sig_len);
void pal_secret_close(pal_secret_t secret);
void pal_pubkey_free(pal_pubkey_t pubkey);

// ── Threading ──────────────────────────────────────────────
// pal_thread_spawn returns the new thread's pthread_t (as int64); an opaque
// token the caller later passes to pal_thread_join. A return of -1 means
// the thread could not be created.
int64_t pal_thread_spawn(void *(*start)(void *), void *arg);
// pal_thread_join blocks until the thread (pthread_t tid) exits and, if
// ret_storage is non-NULL, stores the thread's return value pointer there.
// Returns 0 on success, -1 on error (errno set). Mirrors pthread_join.
int64_t pal_thread_join(int64_t tid, void *ret_storage);

// ── Stateless Services (no handles, no lifecycle) ─────────
int64_t pal_time_now_ms(void);
int64_t pal_time_now_ns(void);
// pal_time_mark / pal_time_unix_ms / pal_time_unix_ns: aliases for
// wall-clock time. pal_time_mark is used by the Mire runtime for
// elapsed-time measurement; pal_time_unix_ms/ns are the standard
// POSIX clock_gettime equivalents.
int64_t pal_time_mark(void);
int64_t pal_time_unix_ms(void);
int64_t pal_time_unix_ns(void);
int64_t pal_cpu_count(void);
// pal_cpu_time_ms: CPU user+system time in milliseconds (not wall clock).
int64_t pal_cpu_time_ms(void);
// pal_cpu_snapshot: returns a map[str i64] with CPU info keys:
// "count", "user_ms", "system_ms", "idle_ms". [PAL-OWNED] caller must free.
const char *pal_cpu_snapshot(void);
int64_t pal_mem_total(void);
int64_t pal_mem_available(void);
int64_t pal_mem_process(void);
// pal_mem_process_bytes: alias for pal_mem_process (process RSS in bytes).
int64_t pal_mem_process_bytes(void);
// pal_mem_format: format a byte count into a human-readable string
// (e.g. "1.5 MiB", "256 KiB"). [PAL-OWNED] caller must free with pal_free().
const char *pal_mem_format(int64_t bytes);
bool pal_random_fill(void *buf, int64_t length);

#endif

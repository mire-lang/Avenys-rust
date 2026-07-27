#ifndef MIRE_PAL_H
#define MIRE_PAL_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

// PAL v4 — Host Resource Model
// ABI contract between Mire and any Host.
// Handles are opaque: {index, generation}. Only PAL Core knows the slot table.
// Host Adapters receive internal data through Core dispatch, not through handle fields.

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

typedef struct {
    void *data;
    int64_t len;
} pal_bytes_t;

// ── Directory Entry (PAL-owned) ─────────────────────────────

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

// ── Filesystem (absolute path operations) ────────────────────
bool pal_fs_exists(const char *path);
bool pal_fs_mkdir(const char *path);
bool pal_fs_rmdir(const char *path);
bool pal_fs_unlink(const char *path);
const char *pal_fs_read_file(const char *path);

// ── Environment ──────────────────────────────────────────────
const char *pal_env_cwd(void);
const char *pal_env_get(const char *name);

// ── Process convenience (legacy compatibility) ───────────────
// pal_proc_system: blocking shell execution via system(). Never use in
// production — shell injection surface. Use pal_proc_create +
// pal_proc_wait for safe execution.
// pal_proc_wait_pid: PID-based waitpid for rare cases where you only
// have a raw OS PID (e.g. from pal_proc_spawn's fork). Prefer
// pal_proc_wait(pal_process_t) which is handle-based, validated, and safe.
int64_t pal_proc_system(const char *cmd);
int64_t pal_proc_capture(const char *cmd, void *buf, int64_t capacity);
const char *pal_proc_capture_output(const char *cmd);
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

// ── Networking ───────────────────────────────────────────────
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

// ── Crypto ───────────────────────────────────────────────────
pal_secret_t pal_secret_create(pal_crypto_algorithm_t algorithm);
pal_pubkey_t pal_secret_export_public(pal_secret_t secret);
int64_t pal_secret_sign(pal_secret_t secret, const void *msg, int64_t msg_len,
                        void *buf, int64_t capacity);
bool pal_pubkey_verify(pal_pubkey_t pubkey, const void *msg, int64_t msg_len,
                       const void *sig, int64_t sig_len);
void pal_secret_close(pal_secret_t secret);
void pal_pubkey_free(pal_pubkey_t pubkey);

// ── Threading ────────────────────────────────────────────────
int64_t pal_thread_spawn(void *(*start)(void *), void *arg);

// ── Stateless Services (no handles, no lifecycle) ────────────
int64_t pal_time_now_ms(void);
int64_t pal_time_now_ns(void);
int64_t pal_cpu_count(void);
int64_t pal_mem_total(void);
int64_t pal_mem_available(void);
int64_t pal_mem_process(void);
bool pal_random_fill(void *buf, int64_t length);

#endif

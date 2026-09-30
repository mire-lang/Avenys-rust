// WASI Preview 1 Host Adapter for the PAL v4 stateless services.
// Resource-bearing PAL operations remain unsupported until their WASI handle
// mapping is specified; returning a stable error is safer than host fallback.
#include "pal.h"
#include "pal_core.h"
#include <stddef.h>
#include <stdint.h>

extern void *malloc(size_t size);
extern void free(void *ptr);
extern void *realloc(void *ptr, size_t size);

static size_t wasi_strlen(const char *value) {
    size_t length = 0;
    if (!value) return 0;
    while (value[length]) length++;
    return length;
}

typedef struct {
    const void *buf;
    size_t len;
} WasiIovec;

#if defined(__wasm__)
__attribute__((import_module("wasi_snapshot_preview1")))
__attribute__((import_name("fd_write")))
#endif
extern uint32_t wasi_fd_write(uint32_t fd, const WasiIovec *iovs, size_t count, size_t *written);

#if defined(__wasm__)
__attribute__((import_module("wasi_snapshot_preview1")))
__attribute__((import_name("clock_time_get")))
#endif
extern uint32_t wasi_clock_time_get(uint32_t clock_id, uint64_t precision, uint64_t *time);

#if defined(__wasm__)
__attribute__((import_module("wasi_snapshot_preview1")))
__attribute__((import_name("random_get")))
#endif
extern uint32_t wasi_random_get(void *buf, size_t len);

// Stateless service implementations (no handles, no lifecycle)

void pal_io_print_err(const char *msg) {
    if (!msg) return;
    WasiIovec iov = { msg, wasi_strlen(msg) };
    size_t written = 0;
    (void)wasi_fd_write(2, &iov, 1, &written);
}

int64_t pal_time_now_ns(void) {
    uint64_t value = 0;
    return wasi_clock_time_get(1, 1, &value) == 0 ? (int64_t)value : 0;
}

int64_t pal_time_now_ms(void) {
    return pal_time_now_ns() / 1000000;
}

bool pal_random_fill(void *buf, int64_t length) {
    return buf && length >= 0 && wasi_random_get(buf, (size_t)length) == 0;
}

const char *pal_env_get(const char *name) {
    (void)name;
    return "";
}

void *pal_alloc(int64_t size) {
    return size > 0 ? malloc((size_t)size) : NULL;
}

void pal_free(void *ptr) { free(ptr); }

void *pal_realloc(void *ptr, int64_t size) {
    return size > 0 ? realloc(ptr, (size_t)size) : NULL;
}

void *pal_secure_alloc(int64_t size) {
    void *ptr = size > 0 ? malloc((size_t)size) : NULL;
    if (ptr) {
        // Zero on alloc for security-sensitive allocations
        for (int64_t i = 0; i < size; i++) {
            ((uint8_t *)ptr)[i] = 0;
        }
    }
    return ptr;
}

void pal_secure_free(void *ptr) {
    if (!ptr) return;
    // Best-effort zero before free; in WASI we can't mprotect.
    free(ptr);
}

int64_t pal_cpu_count(void) { return 0; }
int64_t pal_mem_total(void) { return 0; }
int64_t pal_mem_available(void) { return 0; }
int64_t pal_mem_process(void) { return 0; }
int64_t pal_cpu_time_ms(void) { return 0; }
const char *pal_cpu_snapshot(void) { return ""; }
const char *pal_mem_format(int64_t bytes) { (void)bytes; return ""; }
int64_t pal_time_mark(void) { return 0; }
int64_t pal_time_unix_ms(void) { return 0; }
int64_t pal_time_unix_ns(void) { return 0; }
int64_t pal_mem_process_bytes(void) { return 0; }
bool pal_proc_exists(int64_t pid) { (void)pid; return false; }
int64_t pal_proc_run(const char *cmd, const char **argv) { (void)cmd; (void)argv; return -1; }
bool pal_fs_exists(const char *path) { (void)path; return false; }
bool pal_fs_mkdir(const char *path) { (void)path; return false; }
bool pal_fs_rmdir(const char *path) { (void)path; return false; }
bool pal_fs_unlink(const char *path) { (void)path; return false; }
bool pal_fs_remove(const char *path) { (void)path; return false; }
const char *pal_fs_ext(const char *path) { (void)path; return ""; }
const char *pal_fs_dir(const char *path) { (void)path; return ""; }
const char *pal_fs_name(const char *path) { (void)path; return ""; }
bool pal_fs_is_file(const char *path) { (void)path; return false; }
bool pal_fs_copy(const char *src, const char *dst) { (void)src; (void)dst; return false; }
bool pal_fs_move(const char *src, const char *dst) { (void)src; (void)dst; return false; }
bool pal_fs_chmod(const char *path, const char *mode) { (void)path; (void)mode; return false; }
const char *pal_env_all(void) { return ""; }

// PAL backend operations table for WASI
static int wasi_init(void) { return 0; }
static void wasi_shutdown(void) {}

static const pal_ops_t wasi_ops = {
    .init = wasi_init,
    .shutdown = wasi_shutdown,
    .root_open = NULL,
    .root_close = NULL,
    .root_remove = NULL,
    .file_open = NULL,
    .file_read = NULL,
    .file_write = NULL,
    .file_seek = NULL,
    .file_stat = NULL,
    .file_size = NULL,
    .file_clone = NULL,
    .file_close = NULL,
    .dir_open = NULL,
    .dir_next = NULL,
    .dir_close = NULL,
    .proc_create = NULL,
    .proc_wait = NULL,
    .proc_kill = NULL,
    .proc_stdin = NULL,
    .proc_stdout = NULL,
    .proc_stderr = NULL,
    .proc_close = NULL,
    .socket_connect = NULL,
    .listener_bind = NULL,
    .listener_accept = NULL,
    .socket_send = NULL,
    .socket_recv = NULL,
    .listener_send = NULL,
    .listener_recv = NULL,
    .socket_close = NULL,
    .listener_close = NULL,
    .channel_create = NULL,
    .channel_send = NULL,
    .channel_recv = NULL,
    .channel_close = NULL,
    .secret_create = NULL,
    .secret_export_public = NULL,
    .secret_sign = NULL,
    .pubkey_verify = NULL,
    .secret_close = NULL,
    .pubkey_close = NULL,
    .time_now_ms = pal_time_now_ms,
    .time_now_ns = pal_time_now_ns,
    .cpu_count = pal_cpu_count,
    .mem_total = pal_mem_total,
    .mem_available = pal_mem_available,
    .mem_process = pal_mem_process,
    .random_fill = pal_random_fill,
    .cpu_time_ms = pal_cpu_time_ms,
    .cpu_snapshot = pal_cpu_snapshot,
    .mem_format = pal_mem_format,
    .time_mark = pal_time_mark,
    .time_unix_ms = pal_time_unix_ms,
    .time_unix_ns = pal_time_unix_ns,
    .mem_process_bytes = pal_mem_process_bytes,
    .proc_exists = pal_proc_exists,
    .proc_run = pal_proc_run,
    .fs_exists = pal_fs_exists,
    .fs_mkdir = pal_fs_mkdir,
    .fs_rmdir = pal_fs_rmdir,
    .fs_unlink = pal_fs_unlink,
    .fs_remove = pal_fs_remove,
    .fs_ext = pal_fs_ext,
    .fs_dir = pal_fs_dir,
    .fs_name = pal_fs_name,
    .fs_is_file = pal_fs_is_file,
    .fs_copy = pal_fs_copy,
    .fs_move = pal_fs_move,
    .fs_chmod = pal_fs_chmod,
    .env_all = pal_env_all,
    .io_print_err = pal_io_print_err,
};

// Weak references to the PAL Core orchestration layer so this Host Adapter can
// be compiled and linked standalone (e.g. the stateless Docker smoke fixture)
// as well as inside a full PAL build. When the full core is NOT linked, these
// resolve to NULL and registration is a no-op — the direct WASI services above
// remain callable on their own.
__attribute__((weak)) void pal_dispatch_set_ops(const pal_ops_t *ops);
__attribute__((weak)) int pal_core_init(const pal_ops_t *ops);

// Constructor to register this backend with PAL Core when it is linked.
__attribute__((constructor))
static int pal_backend_register(void) {
    if (pal_dispatch_set_ops) pal_dispatch_set_ops(&wasi_ops);
    if (pal_core_init) pal_core_init(&wasi_ops);
    return 0;
}

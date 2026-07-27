#ifndef MIRE_PAL_CORE_H
#define MIRE_PAL_CORE_H

#include "pal_abi.h"

// ── Resource Types ───────────────────────────────────────────

typedef enum {
    PAL_RES_ANY = -1,
    PAL_RES_ROOT,
    PAL_RES_FILE,
    PAL_RES_DIR,
    PAL_RES_SOCKET,
    PAL_RES_LISTENER,
    PAL_RES_CHANNEL,
    PAL_RES_PROCESS,
    PAL_RES_SECRET,
    PAL_RES_PUBKEY,
} pal_resource_type_t;

// ── Handle Slot Table ────────────────────────────────────────

#define PAL_MAX_SLOTS 4096

typedef struct {
    uint32_t index;
    uint32_t generation;
    bool in_use;
    pal_resource_type_t type;
    void *internal;
    int64_t owner_thread;
} pal_slot_t;

extern pal_slot_t g_slots[PAL_MAX_SLOTS];

// ── Backend Operations Table ─────────────────────────────────
// Host Adapter fills this. PAL Core dispatches through it.

typedef struct pal_ops {
    // Lifecycle
    int (*init)(void);
    void (*shutdown)(void);

    // Root
    int64_t (*root_open)(const char *path);
    void (*root_close)(int64_t internal);

    // File
    int64_t (*file_open)(int64_t root_internal, const char *rel_path, pal_open_flags flags);
    int64_t (*file_read)(int64_t internal, void *buf, int64_t capacity);
    int64_t (*file_write)(int64_t internal, const void *buf, int64_t length);
    int64_t (*file_seek)(int64_t internal, int64_t offset, pal_seek_from_t from);
    bool (*file_stat)(int64_t internal, pal_stat_t *out);
    int64_t (*file_size)(int64_t internal);
    int64_t (*file_clone)(int64_t internal);
    void (*file_close)(int64_t internal);

    // Directory
    int64_t (*dir_open)(int64_t root_internal, const char *rel_path);
    bool (*dir_next)(int64_t internal, pal_dir_entry_t *out);
    void (*dir_close)(int64_t internal);

    // Process
    int64_t (*proc_create)(const char **argv, pal_spawn_flags flags,
                            int64_t stdin_internal, int64_t stdout_internal,
                            int64_t stderr_internal);
    int64_t (*proc_wait)(int64_t internal);
    bool (*proc_kill)(int64_t internal);
    int64_t (*proc_stdin)(int64_t internal);
    int64_t (*proc_stdout)(int64_t internal);
    int64_t (*proc_stderr)(int64_t internal);
    void (*proc_close)(int64_t internal);

    // Networking
    int64_t (*socket_connect)(const char *host, uint16_t port, pal_socket_flags flags);
    int64_t (*listener_bind)(uint16_t port, pal_socket_flags flags);
    int64_t (*listener_accept)(int64_t listener_internal);
    int64_t (*socket_send)(int64_t internal, const void *buf, int64_t length);
    int64_t (*socket_recv)(int64_t internal, void *buf, int64_t capacity);
    void (*socket_close)(int64_t internal);
    void (*listener_close)(int64_t internal);

    // Channels
    int64_t (*channel_create)(void);
    int64_t (*channel_send)(int64_t internal, const void *buf, int64_t length);
    bool (*channel_recv)(int64_t internal, pal_bytes_t *out);
    void (*channel_close)(int64_t internal);

    // Crypto
    int64_t (*secret_create)(pal_crypto_algorithm_t algorithm);
    int64_t (*secret_export_public)(int64_t secret_internal);
    int64_t (*secret_sign)(int64_t secret_internal, const void *msg, int64_t msg_len,
                           void *buf, int64_t capacity);
    bool (*pubkey_verify)(int64_t pubkey_internal, const void *msg, int64_t msg_len,
                          const void *sig, int64_t sig_len);
    void (*secret_close)(int64_t internal);
    void (*pubkey_close)(int64_t internal);

    // Stateless services (no handles)
    int64_t (*time_now_ms)(void);
    int64_t (*time_now_ns)(void);
    int64_t (*cpu_count)(void);
    int64_t (*mem_total)(void);
    int64_t (*mem_available)(void);
    int64_t (*mem_process)(void);
    bool (*random_fill)(void *buf, int64_t length);
} pal_ops_t;

// ── Core API ─────────────────────────────────────────────────

int pal_core_init(const pal_ops_t *ops);
void pal_core_shutdown(void);

// Dispatch layer setup (called by Host Adapter registration)
void pal_dispatch_set_ops(const pal_ops_t *ops);

// Handle management
int pal_core_reserve(pal_resource_type_t type);
void pal_core_release(int64_t slot);
int pal_core_validate(int64_t slot, uint32_t generation, pal_resource_type_t expected_type);
void pal_core_transfer(int64_t slot);

// Internal data access
void *pal_core_get_internal(int64_t slot);

// Error state
void pal_set_error(pal_error_code_t code, const char *message);

#endif

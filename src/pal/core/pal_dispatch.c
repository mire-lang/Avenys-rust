#include "pal_core.h"
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdio.h>

// ── PAL Dispatch Layer ───────────────────────────────────────
// This file implements the public ABI functions declared in pal.h.
// Each function validates the handle, extracts internal data from the
// slot table, and calls the backend ops.
//
// This is the ONLY place where the ABI surface is implemented.
// Host Adapters implement only the backend ops.

static const pal_ops_t *ops;

static int dispatch_initialized;

extern int pal_backend_register(void);

static void pal_ensure_init(void) {
    if (!dispatch_initialized) {
        dispatch_initialized = 1;
        pal_backend_register();
    }
}

// ── Init (called by Host Adapter registration) ───────────────

void pal_dispatch_set_ops(const pal_ops_t *backend_ops) {
    ops = backend_ops;
    dispatch_initialized = 1;
}

// ── Filesystem ──────────────────────────────────────────────

pal_root_t pal_root_open(const char *path) {
    pal_ensure_init();
    if (!ops || !ops->root_open) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_ROOT_NULL; }
    int64_t internal = ops->root_open(path);
    if (internal <= 0) { pal_set_error(PAL_ERR_IO, "root_open failed"); return PAL_ROOT_NULL; }
    int slot = pal_core_reserve(PAL_RES_ROOT);
    if (slot < 0) { pal_set_error(PAL_ERR_NO_MEM, "no slots"); return PAL_ROOT_NULL; }
    // Store internal in slot
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)internal;
    pal_root_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

void pal_root_close(pal_root_t root) {
    if (!ops || !ops->root_close) return;
    int64_t internal = (int64_t)pal_core_get_internal(root.index);
    if (internal) ops->root_close(internal);
    pal_core_release(root.index);
}

pal_file_t pal_file_open(pal_root_t root, const char *rel_path, pal_open_flags flags) {
    if (!ops || !ops->file_open) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_FILE_NULL; }
    if (!pal_core_validate(root.index, root.generation, PAL_RES_ROOT)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad root handle"); return PAL_FILE_NULL; }
    int64_t root_internal = (int64_t)pal_core_get_internal(root.index);
    int64_t internal = ops->file_open(root_internal, rel_path, flags);
    if (internal <= 0) { pal_set_error(PAL_ERR_IO, "file_open failed"); return PAL_FILE_NULL; }
    int slot = pal_core_reserve(PAL_RES_FILE);
    if (slot < 0) { pal_set_error(PAL_ERR_NO_MEM, "no slots"); return PAL_FILE_NULL; }
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)internal;
    pal_file_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

int64_t pal_file_read(pal_file_t file, void *buf, int64_t capacity) {
    if (!ops || !ops->file_read) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return -1; }
    if (!pal_core_validate(file.index, file.generation, PAL_RES_FILE)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad file handle"); return -1; }
    int64_t internal = (int64_t)pal_core_get_internal(file.index);
    return ops->file_read(internal, buf, capacity);
}

int64_t pal_file_write(pal_file_t file, const void *buf, int64_t length) {
    if (!ops || !ops->file_write) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return -1; }
    if (!pal_core_validate(file.index, file.generation, PAL_RES_FILE)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad file handle"); return -1; }
    int64_t internal = (int64_t)pal_core_get_internal(file.index);
    return ops->file_write(internal, buf, length);
}

int64_t pal_file_seek(pal_file_t file, int64_t offset, pal_seek_from_t from) {
    if (!ops || !ops->file_seek) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return -1; }
    if (!pal_core_validate(file.index, file.generation, PAL_RES_FILE)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad file handle"); return -1; }
    int64_t internal = (int64_t)pal_core_get_internal(file.index);
    return ops->file_seek(internal, offset, from);
}

bool pal_file_stat(pal_file_t file, pal_stat_t *out) {
    if (!ops || !ops->file_stat) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return false; }
    if (!pal_core_validate(file.index, file.generation, PAL_RES_FILE)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad file handle"); return false; }
    int64_t internal = (int64_t)pal_core_get_internal(file.index);
    return ops->file_stat(internal, out);
}

int64_t pal_file_size(pal_file_t file) {
    if (!ops || !ops->file_size) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return -1; }
    if (!pal_core_validate(file.index, file.generation, PAL_RES_FILE)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad file handle"); return -1; }
    int64_t internal = (int64_t)pal_core_get_internal(file.index);
    return ops->file_size(internal);
}

pal_file_t pal_file_clone(pal_file_t file) {
    if (!ops || !ops->file_clone) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_FILE_NULL; }
    if (!pal_core_validate(file.index, file.generation, PAL_RES_FILE)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad file handle"); return PAL_FILE_NULL; }
    int64_t internal = (int64_t)pal_core_get_internal(file.index);
    int64_t cloned = ops->file_clone(internal);
    if (cloned <= 0) return PAL_FILE_NULL;
    int slot = pal_core_reserve(PAL_RES_FILE);
    if (slot < 0) return PAL_FILE_NULL;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)cloned;
    pal_file_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

void pal_file_close(pal_file_t file) {
    if (!ops || !ops->file_close) return;
    if (pal_core_validate(file.index, file.generation, PAL_RES_FILE)) {
        int64_t internal = (int64_t)pal_core_get_internal(file.index);
        if (internal) ops->file_close(internal);
    }
    pal_core_release(file.index);
}

// ── Directory ────────────────────────────────────────────────

pal_dir_t pal_dir_open(pal_root_t root, const char *rel_path) {
    if (!ops || !ops->dir_open) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_DIR_NULL; }
    if (!pal_core_validate(root.index, root.generation, PAL_RES_ROOT)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad root handle"); return PAL_DIR_NULL; }
    int64_t root_internal = (int64_t)pal_core_get_internal(root.index);
    int64_t internal = ops->dir_open(root_internal, rel_path);
    if (internal <= 0) { pal_set_error(PAL_ERR_IO, "dir_open failed"); return PAL_DIR_NULL; }
    int slot = pal_core_reserve(PAL_RES_DIR);
    if (slot < 0) return PAL_DIR_NULL;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)internal;
    pal_dir_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

pal_dir_entry_t pal_dir_next(pal_dir_t dir) {
    pal_dir_entry_t entry = {0};
    if (!ops || !ops->dir_next) return entry;
    if (!pal_core_validate(dir.index, dir.generation, PAL_RES_DIR)) return entry;
    int64_t internal = (int64_t)pal_core_get_internal(dir.index);
    ops->dir_next(internal, &entry);
    return entry;
}

void pal_dir_next_into(pal_dir_t dir, pal_dir_entry_t *out) {
    if (!out) return;
    memset(out, 0, sizeof(pal_dir_entry_t));
    if (!ops || !ops->dir_next) return;
    if (!pal_core_validate(dir.index, dir.generation, PAL_RES_DIR)) return;
    int64_t internal = (int64_t)pal_core_get_internal(dir.index);
    ops->dir_next(internal, out);
}

int64_t pal_dir_next_name(pal_dir_t dir, char *out, int64_t cap) {
    if (!out || cap <= 0) return -1;
    out[0] = '\0';
    if (!ops || !ops->dir_next) return -1;
    if (!pal_core_validate(dir.index, dir.generation, PAL_RES_DIR)) return -1;
    int64_t internal = (int64_t)pal_core_get_internal(dir.index);
    pal_dir_entry_t entry = {0};
    ops->dir_next(internal, &entry);
    if (entry.name[0] == '\0') return 0;
    int64_t len = strlen(entry.name);
    if (len >= cap) len = cap - 1;
    memcpy(out, entry.name, len);
    out[len] = '\0';
    return len;
}

void pal_dir_close(pal_dir_t dir) {
    if (!ops || !ops->dir_close) return;
    if (pal_core_validate(dir.index, dir.generation, PAL_RES_DIR)) {
        int64_t internal = (int64_t)pal_core_get_internal(dir.index);
        if (internal) ops->dir_close(internal);
    }
    pal_core_release(dir.index);
}

// ── Process ──────────────────────────────────────────────────

pal_process_t pal_proc_create(const char **argv, pal_spawn_flags flags,
                              pal_channel_t stdin_ch, pal_channel_t stdout_ch,
                              pal_channel_t stderr_ch) {
    if (!ops || !ops->proc_create) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_PROCESS_NULL; }
    int64_t stdin_internal = (int64_t)pal_core_get_internal(stdin_ch.index);
    int64_t stdout_internal = (int64_t)pal_core_get_internal(stdout_ch.index);
    int64_t stderr_internal = (int64_t)pal_core_get_internal(stderr_ch.index);
    int64_t internal = ops->proc_create(argv, flags, stdin_internal, stdout_internal, stderr_internal);
    if (internal <= 0) { pal_set_error(PAL_ERR_IO, "proc_create failed"); return PAL_PROCESS_NULL; }
    int slot = pal_core_reserve(PAL_RES_PROCESS);
    if (slot < 0) return PAL_PROCESS_NULL;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)internal;
    pal_process_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

int64_t pal_proc_wait(pal_process_t proc) {
    if (!ops || !ops->proc_wait) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return -1; }
    if (!pal_core_validate(proc.index, proc.generation, PAL_RES_PROCESS)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad process handle"); return -1; }
    int64_t internal = (int64_t)pal_core_get_internal(proc.index);
    return ops->proc_wait(internal);
}

bool pal_proc_kill(pal_process_t proc) {
    if (!ops || !ops->proc_kill) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return false; }
    if (!pal_core_validate(proc.index, proc.generation, PAL_RES_PROCESS)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad process handle"); return false; }
    int64_t internal = (int64_t)pal_core_get_internal(proc.index);
    return ops->proc_kill(internal);
}

pal_channel_t pal_proc_stdin(pal_process_t proc) {
    pal_channel_t ch = PAL_CHANNEL_NULL;
    if (!ops || !ops->proc_stdin) return ch;
    if (!pal_core_validate(proc.index, proc.generation, PAL_RES_PROCESS)) return ch;
    int64_t internal = (int64_t)pal_core_get_internal(proc.index);
    int64_t ch_internal = ops->proc_stdin(internal);
    if (ch_internal <= 0) return ch;
    int slot = pal_core_reserve(PAL_RES_CHANNEL);
    if (slot < 0) return ch;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)ch_internal;
    ch.index = (uint32_t)slot;
    ch.generation = g_slots[slot].generation;
    return ch;
}

pal_channel_t pal_proc_stdout(pal_process_t proc) {
    pal_channel_t ch = PAL_CHANNEL_NULL;
    if (!ops || !ops->proc_stdout) return ch;
    if (!pal_core_validate(proc.index, proc.generation, PAL_RES_PROCESS)) return ch;
    int64_t internal = (int64_t)pal_core_get_internal(proc.index);
    int64_t ch_internal = ops->proc_stdout(internal);
    if (ch_internal <= 0) return ch;
    int slot = pal_core_reserve(PAL_RES_CHANNEL);
    if (slot < 0) return ch;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)ch_internal;
    ch.index = (uint32_t)slot;
    ch.generation = g_slots[slot].generation;
    return ch;
}

pal_channel_t pal_proc_stderr(pal_process_t proc) {
    pal_channel_t ch = PAL_CHANNEL_NULL;
    if (!ops || !ops->proc_stderr) return ch;
    if (!pal_core_validate(proc.index, proc.generation, PAL_RES_PROCESS)) return ch;
    int64_t internal = (int64_t)pal_core_get_internal(proc.index);
    int64_t ch_internal = ops->proc_stderr(internal);
    if (ch_internal <= 0) return ch;
    int slot = pal_core_reserve(PAL_RES_CHANNEL);
    if (slot < 0) return ch;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)ch_internal;
    ch.index = (uint32_t)slot;
    ch.generation = g_slots[slot].generation;
    return ch;
}

pal_process_t pal_proc_transfer(pal_process_t proc) {
    if (pal_core_validate(proc.index, proc.generation, PAL_RES_PROCESS)) {
        pal_core_transfer(proc.index);
    }
    return proc;
}

void pal_proc_close(pal_process_t proc) {
    if (!ops || !ops->proc_close) return;
    if (pal_core_validate(proc.index, proc.generation, PAL_RES_PROCESS)) {
        int64_t internal = (int64_t)pal_core_get_internal(proc.index);
        if (internal) ops->proc_close(internal);
    }
    pal_core_release(proc.index);
}

// ── Networking ───────────────────────────────────────────────

pal_socket_t pal_socket_connect(const char *host, uint16_t port, pal_socket_flags flags) {
    if (!ops || !ops->socket_connect) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_SOCKET_NULL; }
    int64_t internal = ops->socket_connect(host, port, flags);
    if (internal <= 0) { pal_set_error(PAL_ERR_IO, "connect failed"); return PAL_SOCKET_NULL; }
    int slot = pal_core_reserve(PAL_RES_SOCKET);
    if (slot < 0) return PAL_SOCKET_NULL;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)internal;
    pal_socket_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

pal_listener_t pal_listener_bind(uint16_t port, pal_socket_flags flags) {
    if (!ops || !ops->listener_bind) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_LISTENER_NULL; }
    int64_t internal = ops->listener_bind(port, flags);
    if (internal <= 0) { pal_set_error(PAL_ERR_IO, "bind failed"); return PAL_LISTENER_NULL; }
    int slot = pal_core_reserve(PAL_RES_LISTENER);
    if (slot < 0) return PAL_LISTENER_NULL;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)internal;
    pal_listener_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

pal_socket_t pal_listener_accept(pal_listener_t listener) {
    if (!ops || !ops->listener_accept) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_SOCKET_NULL; }
    int64_t internal = (int64_t)pal_core_get_internal(listener.index);
    if (!internal) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad handle"); return PAL_SOCKET_NULL; }
    int64_t accepted = ops->listener_accept(internal);
    if (accepted <= 0) { pal_set_error(PAL_ERR_IO, "accept failed"); return PAL_SOCKET_NULL; }
    int slot = pal_core_reserve(PAL_RES_SOCKET);
    if (slot < 0) return PAL_SOCKET_NULL;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)accepted;
    pal_socket_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

int64_t pal_socket_send(pal_socket_t sock, const void *buf, int64_t length) {
    if (!ops || !ops->socket_send) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return -1; }
    int64_t internal = (int64_t)pal_core_get_internal(sock.index);
    if (!internal) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad handle"); return -1; }
    return ops->socket_send(internal, buf, length);
}

int64_t pal_socket_recv(pal_socket_t sock, void *buf, int64_t capacity) {
    if (!ops || !ops->socket_recv) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return -1; }
    int64_t internal = (int64_t)pal_core_get_internal(sock.index);
    if (!internal) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad handle"); return -1; }
    return ops->socket_recv(internal, buf, capacity);
}

void pal_socket_close(pal_socket_t sock) {
    if (!ops || !ops->socket_close) return;
    int64_t internal = (int64_t)pal_core_get_internal(sock.index);
    if (internal) ops->socket_close(internal);
    pal_core_release(sock.index);
}

void pal_listener_close(pal_listener_t listener) {
    if (!ops || !ops->listener_close) return;
    int64_t internal = (int64_t)pal_core_get_internal(listener.index);
    if (internal) ops->listener_close(internal);
    pal_core_release(listener.index);
}

// ── Channels ─────────────────────────────────────────────────

pal_channel_t pal_channel_create(void) {
    if (!ops || !ops->channel_create) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_CHANNEL_NULL; }
    int64_t internal = ops->channel_create();
    if (internal <= 0) { pal_set_error(PAL_ERR_IO, "channel_create failed"); return PAL_CHANNEL_NULL; }
    int slot = pal_core_reserve(PAL_RES_CHANNEL);
    if (slot < 0) return PAL_CHANNEL_NULL;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)internal;
    pal_channel_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

int64_t pal_channel_send(pal_channel_t ch, const void *buf, int64_t length) {
    if (!ops || !ops->channel_send) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return -1; }
    int64_t internal = (int64_t)pal_core_get_internal(ch.index);
    if (!internal) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad handle"); return -1; }
    return ops->channel_send(internal, buf, length);
}

pal_bytes_t pal_channel_recv(pal_channel_t ch) {
    pal_bytes_t result = {0};
    if (!ops || !ops->channel_recv) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return result; }
    int64_t internal = (int64_t)pal_core_get_internal(ch.index);
    if (!internal) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad handle"); return result; }
    ops->channel_recv(internal, &result);
    return result;
}

void pal_channel_close(pal_channel_t ch) {
    if (!ops || !ops->channel_close) return;
    int64_t internal = (int64_t)pal_core_get_internal(ch.index);
    if (internal) ops->channel_close(internal);
    pal_core_release(ch.index);
}

// ── Crypto ───────────────────────────────────────────────────

pal_secret_t pal_secret_create(pal_crypto_algorithm_t algorithm) {
    if (!ops || !ops->secret_create) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_SECRET_NULL; }
    int64_t internal = ops->secret_create(algorithm);
    if (internal <= 0) { pal_set_error(PAL_ERR_IO, "secret_create failed"); return PAL_SECRET_NULL; }
    int slot = pal_core_reserve(PAL_RES_SECRET);
    if (slot < 0) return PAL_SECRET_NULL;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)internal;
    pal_secret_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

pal_pubkey_t pal_secret_export_public(pal_secret_t secret) {
    if (!ops || !ops->secret_export_public) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return PAL_PUBKEY_NULL; }
    if (!pal_core_validate(secret.index, secret.generation, PAL_RES_SECRET)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad secret handle"); return PAL_PUBKEY_NULL; }
    int64_t internal = (int64_t)pal_core_get_internal(secret.index);
    int64_t pk = ops->secret_export_public(internal);
    if (pk <= 0) return PAL_PUBKEY_NULL;
    int slot = pal_core_reserve(PAL_RES_PUBKEY);
    if (slot < 0) return PAL_PUBKEY_NULL;
    extern pal_slot_t g_slots[];
    g_slots[slot].internal = (void *)pk;
    pal_pubkey_t h = { .index = (uint32_t)slot, .generation = g_slots[slot].generation };
    return h;
}

int64_t pal_secret_sign(pal_secret_t secret, const void *msg, int64_t msg_len,
                        void *buf, int64_t capacity) {
    if (!ops || !ops->secret_sign) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return -1; }
    if (!pal_core_validate(secret.index, secret.generation, PAL_RES_SECRET)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad secret handle"); return -1; }
    int64_t internal = (int64_t)pal_core_get_internal(secret.index);
    return ops->secret_sign(internal, msg, msg_len, buf, capacity);
}

bool pal_pubkey_verify(pal_pubkey_t pubkey, const void *msg, int64_t msg_len,
                       const void *sig, int64_t sig_len) {
    if (!ops || !ops->pubkey_verify) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return false; }
    if (!pal_core_validate(pubkey.index, pubkey.generation, PAL_RES_PUBKEY)) { pal_set_error(PAL_ERR_INVALID_HANDLE, "bad pubkey handle"); return false; }
    int64_t internal = (int64_t)pal_core_get_internal(pubkey.index);
    return ops->pubkey_verify(internal, msg, msg_len, sig, sig_len);
}

void pal_secret_close(pal_secret_t secret) {
    if (!ops || !ops->secret_close) return;
    if (pal_core_validate(secret.index, secret.generation, PAL_RES_SECRET)) {
        int64_t internal = (int64_t)pal_core_get_internal(secret.index);
        if (internal) ops->secret_close(internal);
    }
    pal_core_release(secret.index);
}

void pal_pubkey_free(pal_pubkey_t pubkey) {
    if (!ops || !ops->pubkey_close) return;
    if (pal_core_validate(pubkey.index, pubkey.generation, PAL_RES_PUBKEY)) {
        int64_t internal = (int64_t)pal_core_get_internal(pubkey.index);
        if (internal) ops->pubkey_close(internal);
    }
    pal_core_release(pubkey.index);
}

// ── Stateless Services ───────────────────────────────────────

int64_t pal_time_now_ms(void) {
    if (!ops || !ops->time_now_ms) return 0;
    return ops->time_now_ms();
}

int64_t pal_time_now_ns(void) {
    if (!ops || !ops->time_now_ns) return 0;
    return ops->time_now_ns();
}

int64_t pal_cpu_count(void) {
    if (!ops || !ops->cpu_count) return 0;
    return ops->cpu_count();
}

int64_t pal_mem_total(void) {
    if (!ops || !ops->mem_total) return 0;
    return ops->mem_total();
}

int64_t pal_mem_available(void) {
    if (!ops || !ops->mem_available) return 0;
    return ops->mem_available();
}

int64_t pal_mem_process(void) {
    if (!ops || !ops->mem_process) return 0;
    return ops->mem_process();
}

bool pal_random_fill(void *buf, int64_t length) {
    if (!ops || !ops->random_fill) { pal_set_error(PAL_ERR_UNSUPPORTED, "no backend"); return false; }
    return ops->random_fill(buf, length);
}

// ── Thread (legacy runtime compatibility) ─────────────────────

int64_t pal_thread_spawn(void *(*start)(void *), void *arg) {
    pthread_t tid;
    if (pthread_create(&tid, NULL, start, arg) != 0) return -1;
    return (int64_t)tid;
}

// ── Memory ───────────────────────────────────────────────────

void *pal_alloc(int64_t size) {
    if (size <= 0) return NULL;
    return malloc((size_t)size);
}

void pal_free(void *ptr) {
    free(ptr);
}

void *pal_realloc(void *ptr, int64_t new_size) {
    if (new_size <= 0) return NULL;
    return realloc(ptr, (size_t)new_size);
}

void *pal_secure_alloc(int64_t size) {
    if (size <= 0) return NULL;
    if ((uint64_t)size > SIZE_MAX - sizeof(size_t)) return NULL;
    size_t total = sizeof(size_t) + (size_t)size;
    size_t *header = malloc(total);
    if (!header) return NULL;
    *header = (size_t)size;
    void *payload = header + 1;
    memset(payload, 0, (size_t)size);
    return payload;
}

void pal_secure_free(void *ptr) {
    if (!ptr) return;
    size_t *header = ((size_t *)ptr) - 1;
    size_t size = *header;
    volatile unsigned char *bytes = (volatile unsigned char *)header;
    for (size_t i = 0; i < sizeof(*header) + size; i++) bytes[i] = 0;
    free(header);
}

// ── Filesystem (absolute path operations) ────────────────────

bool pal_fs_exists(const char *path) {
    struct stat st;
    return stat(path, &st) == 0;
}

bool pal_fs_mkdir(const char *path) {
    return mkdir(path, 0755) == 0;
}

bool pal_fs_rmdir(const char *path) {
    return rmdir(path) == 0;
}

bool pal_fs_unlink(const char *path) {
    return unlink(path) == 0;
}

const char *pal_fs_read_file(const char *path) {
    if (!path) return "";
    FILE *f = fopen(path, "rb");
    if (!f) return "";
    fseek(f, 0, SEEK_END);
    long len = ftell(f);
    fseek(f, 0, SEEK_SET);
    if (len <= 0) { fclose(f); return ""; }
    char *buf = malloc((size_t)len + 1);
    if (!buf) { fclose(f); return ""; }
    size_t n = fread(buf, 1, (size_t)len, f);
    buf[n] = '\0';
    fclose(f);
    return buf;
}

// ── Environment ──────────────────────────────────────────────

static char g_cwd_buf[4096];

const char *pal_env_cwd(void) {
    if (getcwd(g_cwd_buf, sizeof(g_cwd_buf))) {
        return g_cwd_buf;
    }
    return NULL;
}

const char *pal_env_get(const char *name) {
    if (!name) return NULL;
    return getenv(name);
}

// ── Process convenience (legacy compatibility) ───────────────

static char g_proc_buf[65536];

int64_t pal_proc_system(const char *cmd) {
    if (!cmd) return -1;
    int status = system(cmd);
    if (WIFEXITED(status)) return WEXITSTATUS(status);
    return -1;
}

int64_t pal_proc_capture(const char *cmd, void *buf, int64_t capacity) {
    if (!cmd || !buf || capacity <= 0) return 0;
    FILE *fp = popen(cmd, "r");
    if (!fp) return 0;
    int64_t total = 0;
    char *out = (char *)buf;
    size_t n;
    while (total < capacity - 1 && (n = fread(out + total, 1, (size_t)(capacity - total - 1), fp)) > 0) {
        total += (int64_t)n;
    }
    out[total] = '\0';
    pclose(fp);
    return total;
}

const char *pal_proc_capture_output(const char *cmd) {
    if (!cmd) return "";
    FILE *fp = popen(cmd, "r");
    if (!fp) return "";
    size_t cap = 4096;
    size_t total = 0;
    char *buf = malloc(cap);
    if (!buf) { pclose(fp); return ""; }
    size_t n;
    while ((n = fread(buf + total, 1, cap - total - 1, fp)) > 0) {
        total += n;
        if (total >= cap - 2) {
            cap *= 2;
            char *new_buf = realloc(buf, cap);
            if (!new_buf) { free(buf); pclose(fp); return ""; }
            buf = new_buf;
        }
    }
    buf[total] = '\0';
    pclose(fp);
    return buf;
}

int64_t pal_proc_wait_pid(int64_t pid) {
    if (pid <= 0) return -1;
    int status = 0;
    int ret = waitpid((pid_t)pid, &status, 0);
    if (ret < 0) return -1;
    if (WIFEXITED(status)) return WEXITSTATUS(status);
    return -1;
}

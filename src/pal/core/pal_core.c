#include "pal_core.h"
#include <stdlib.h>
#include <string.h>
#include <pthread.h>

pal_slot_t g_slots[PAL_MAX_SLOTS];
static int g_slot_count;
static pthread_mutex_t g_mutex = PTHREAD_MUTEX_INITIALIZER;
static const pal_ops_t *g_ops;

// ── Init / Shutdown ──────────────────────────────────────────

int pal_core_init(const pal_ops_t *ops) {
    if (!ops) return -1;
    g_ops = ops;
    memset(g_slots, 0, sizeof(g_slots));
    g_slot_count = 0;
    if (g_ops->init) {
        return g_ops->init();
    }
    return 0;
}

void pal_core_shutdown(void) {
    if (g_ops && g_ops->shutdown) {
        g_ops->shutdown();
    }
    g_ops = NULL;
}

// ── Handle Management ────────────────────────────────────────

int pal_core_reserve(pal_resource_type_t type) {
    pthread_mutex_lock(&g_mutex);

    for (int i = 0; i < PAL_MAX_SLOTS; i++) {
        if (!g_slots[i].in_use && !g_slots[i].internal) {
            g_slots[i].index = (uint32_t)i;
            g_slots[i].generation++;
            g_slots[i].in_use = true;
            g_slots[i].type = type;
            g_slots[i].internal = NULL;
            g_slots[i].owner_thread = (int64_t)pthread_self();
            pthread_mutex_unlock(&g_mutex);
            return i;
        }
    }

    pthread_mutex_unlock(&g_mutex);
    return -1;
}

void pal_core_release(int64_t slot) {
    if (slot < 0 || slot >= PAL_MAX_SLOTS) return;

    pthread_mutex_lock(&g_mutex);
    g_slots[slot].in_use = false;
    g_slots[slot].internal = NULL;
    g_slots[slot].owner_thread = 0;
    g_slots[slot].type = PAL_RES_ROOT;
    pthread_mutex_unlock(&g_mutex);
}

int pal_core_validate(int64_t slot, uint32_t generation, pal_resource_type_t expected_type) {
    if (slot < 0 || slot >= PAL_MAX_SLOTS) return 0;
    if (!g_slots[slot].in_use) return 0;
    if (g_slots[slot].generation != generation) return 0;
    if (g_slots[slot].type != expected_type && expected_type != PAL_RES_ANY) return 0;
    if (g_slots[slot].owner_thread != (int64_t)pthread_self()) return 0;
    return 1;
}

void pal_core_transfer(int64_t slot) {
    if (slot < 0 || slot >= PAL_MAX_SLOTS) return;
    pthread_mutex_lock(&g_mutex);
    if (g_slots[slot].in_use) {
        g_slots[slot].owner_thread = (int64_t)pthread_self();
    }
    pthread_mutex_unlock(&g_mutex);
}

void *pal_core_get_internal(int64_t slot) {
    if (slot < 0 || slot >= PAL_MAX_SLOTS) return NULL;
    if (!g_slots[slot].in_use) return NULL;
    return g_slots[slot].internal;
}

void pal_core_set_internal(int64_t slot, void *internal) {
    if (slot < 0 || slot >= PAL_MAX_SLOTS) return;
    g_slots[slot].internal = internal;
}

// ── Handle Validation ────────────────────────────────────────

bool pal_handle_is_valid(pal_handle_t h) {
    return pal_core_validate(h.index, h.generation, PAL_RES_ANY) == 1;
}

// ── Error State ──────────────────────────────────────────────

static __thread pal_error_code_t t_last_error;
static __thread const char *t_last_message;

void pal_set_error(pal_error_code_t code, const char *message) {
    t_last_error = code;
    t_last_message = message;
}

pal_error_code_t pal_last_error(void) {
    return t_last_error;
}

const char *pal_strerror(pal_error_code_t code) {
    switch (code) {
        case PAL_ERR_OK: return "ok";
        case PAL_ERR_NOT_FOUND: return "not found";
        case PAL_ERR_PERMISSION: return "permission denied";
        case PAL_ERR_IO: return "i/o error";
        case PAL_ERR_INVALID: return "invalid argument";
        case PAL_ERR_NO_MEM: return "out of memory";
        case PAL_ERR_BUSY: return "resource busy";
        case PAL_ERR_UNSUPPORTED: return "unsupported operation";
        case PAL_ERR_ALREADY_EXISTS: return "already exists";
        case PAL_ERR_INVALID_HANDLE: return "invalid handle";
        case PAL_ERR_OWNERSHIP: return "ownership violation";
    }
    return "unknown error";
}

void pal_clear_error(void) {
    t_last_error = PAL_ERR_OK;
    t_last_message = NULL;
}

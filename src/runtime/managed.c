#include "runtime.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// ── Managed string tracking (linked list + hash table) ────────────────

typedef struct MireManagedStringNode {
    char *data_ptr;
    struct MireManagedStringNode *next;
} MireManagedStringNode;

static MireManagedStringNode *managed_strings = NULL;

// Hash table for O(1) contains / unregister (replaces linear scan).
#define MANAGED_HT_INITIAL 64

static char **managed_ht_keys = NULL;
static size_t managed_ht_cap = 0;
static size_t managed_ht_len = 0;

static size_t managed_ht_hash(const char *key) {
    size_t h = (size_t)key;
    h ^= h >> 33;
    h *= 0xff51afd7ed558ccdULL;
    h ^= h >> 33;
    return h;
}

static int managed_ht_put(const char *key) {
    if (!managed_ht_keys || managed_ht_len * 2 >= managed_ht_cap) {
        size_t new_cap = managed_ht_cap ? managed_ht_cap * 2 : MANAGED_HT_INITIAL;
        char **new_keys = (char **)calloc(new_cap, sizeof(char *));
        if (!new_keys) return 0;
        for (size_t i = 0; i < managed_ht_cap; i++) {
            if (managed_ht_keys[i]) {
                size_t h = managed_ht_hash(managed_ht_keys[i]);
                for (size_t j = 0; j < new_cap; j++) {
                    size_t idx = (h + j) % new_cap;
                    if (!new_keys[idx]) { new_keys[idx] = managed_ht_keys[i]; break; }
                }
            }
        }
        free(managed_ht_keys);
        managed_ht_keys = new_keys;
        managed_ht_cap = new_cap;
    }
    size_t h = managed_ht_hash(key);
    for (size_t j = 0; j < managed_ht_cap; j++) {
        size_t idx = (h + j) % managed_ht_cap;
        if (!managed_ht_keys[idx]) {
            managed_ht_keys[idx] = (char *)key;
            managed_ht_len++;
            return 1;
        }
        if (managed_ht_keys[idx] == key) return 1;
    }
    return 0;
}

static void managed_ht_remove(const char *key) {
    if (!managed_ht_keys) return;
    size_t h = managed_ht_hash(key);
    for (size_t j = 0; j < managed_ht_cap; j++) {
        size_t idx = (h + j) % managed_ht_cap;
        if (!managed_ht_keys[idx]) return;
        if (managed_ht_keys[idx] == key) {
            managed_ht_keys[idx] = NULL;
            managed_ht_len--;
            return;
        }
    }
}

static int managed_ht_contains(const char *key) {
    if (!managed_ht_keys) return 0;
    size_t h = managed_ht_hash(key);
    for (size_t j = 0; j < managed_ht_cap; j++) {
        size_t idx = (h + j) % managed_ht_cap;
        if (!managed_ht_keys[idx]) return 0;
        if (managed_ht_keys[idx] == key) return 1;
    }
    return 0;
}

void rt_managed_register(char *data_ptr) {
    if (data_ptr == NULL) return;
    MireManagedStringNode *node = (MireManagedStringNode *)malloc(sizeof(MireManagedStringNode));
    if (node == NULL) return;
    node->data_ptr = data_ptr;
    node->next = managed_strings;
    managed_strings = node;
    managed_ht_put(data_ptr);
}

void rt_managed_unregister(char *data_ptr) {
    managed_ht_remove(data_ptr);
    MireManagedStringNode **cursor = &managed_strings;
    while (*cursor != NULL) {
        if ((*cursor)->data_ptr == data_ptr) {
            MireManagedStringNode *node = *cursor;
            *cursor = node->next;
            free(node);
            return;
        }
        cursor = &(*cursor)->next;
    }
}

int rt_managed_contains(const char *data_ptr) {
    return managed_ht_contains(data_ptr);
}

// ── String growth ────────────────────────────────────────────────────

size_t rt_string_growth_cap(size_t min_cap) {
    size_t cap = 16;
    while (cap < min_cap) cap += cap >> 1;
    return cap;
}

static MireManagedString *string_header(char *value) {
    if (value == NULL) return NULL;
    return (MireManagedString *)((char *)value - offsetof(MireManagedString, data));
}

char *rt_strdup_raw(const char *src) {
    size_t len = strlen(src) + 1;
    char *out = (char *)malloc(len);
    if (out == NULL) return NULL;
    memcpy(out, src, len);
    return out;
}

char *rt_strdup_raw_n(const char *src, size_t len) {
    char *out = (char *)malloc(len + 1);
    if (out == NULL) return NULL;
    if (len > 0) memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

char *rt_managed_alloc(size_t len) {
    size_t cap = rt_string_growth_cap(len);
    MireManagedString *header = (MireManagedString *)malloc(sizeof(MireManagedString) + cap + 1);
    if (header == NULL) return NULL;
    header->len = len;
    header->cap = cap;
    header->data[len] = '\0';
    rt_managed_register(header->data);
    return header->data;
}

char *rt_managed_from_slice(const char *src, size_t len) {
    char *out = rt_managed_alloc(len);
    if (out == NULL) return rt_strdup_raw("");
    if (len > 0) memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

char *rt_managed_from_cstr(const char *src) {
    return rt_managed_from_slice(src, strlen(src));
}

char *rt_managed_ensure_managed(char *ptr) {
    if (ptr == NULL) return rt_managed_from_slice("", 0);
    if (rt_managed_contains(ptr)) return ptr;
    return rt_managed_from_cstr(ptr);
}

char *rt_managed_printf_i64(const char *fmt, long long value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) return rt_managed_from_slice("", 0);
    char *out = rt_managed_alloc((size_t)needed);
    if (out == NULL) return rt_managed_from_slice("", 0);
    snprintf(out, (size_t)needed + 1, fmt, value);
    return out;
}

char *rt_managed_printf_f64(const char *fmt, double value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) return rt_managed_from_slice("", 0);
    char *out = rt_managed_alloc((size_t)needed);
    if (out == NULL) return rt_managed_from_slice("", 0);
    snprintf(out, (size_t)needed + 1, fmt, value);
    return out;
}

char *rt_alloc_printf_raw_i64(const char *fmt, long long value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) return rt_strdup_raw("");
    char *out = (char *)malloc((size_t)needed + 1);
    if (out == NULL) return rt_strdup_raw("");
    snprintf(out, (size_t)needed + 1, fmt, value);
    return out;
}

void rt_managed_free(char *value) {
    if (value == NULL) return;
    if (!rt_managed_contains(value)) {
        return;
    }
    rt_managed_unregister(value);
    MireManagedString *header = string_header(value);
    free(header);
}

void rt_managed_cleanup_all(void) {
    MireManagedStringNode *node = managed_strings;
    while (node != NULL) {
        MireManagedStringNode *next = node->next;
        MireManagedString *header = string_header(node->data_ptr);
        if (header) free(header);
        free(node);
        node = next;
    }
    managed_strings = NULL;
    free(managed_ht_keys);
    managed_ht_keys = NULL;
    managed_ht_cap = 0;
    managed_ht_len = 0;
}

size_t rt_managed_len(const char *value) {
    if (value == NULL) return 0;
    MireManagedString *header = string_header((char *)value);
    return header ? header->len : strlen(value);
}

// ── Runtime utilities ────────────────────────────────────────────────

void rt_panic(const char *message) {
    if (message && *message) {
        fprintf(stderr, "runtime error: %s\n", message);
    } else {
        fprintf(stderr, "runtime error\n");
    }
    fflush(stderr);
    exit(101);
}

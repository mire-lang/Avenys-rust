#include "runtime.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// ── Managed string tracking (linked list of allocated strings) ───────

typedef struct MireManagedStringNode {
    char *data_ptr;
    struct MireManagedStringNode *next;
} MireManagedStringNode;

static MireManagedStringNode *managed_strings = NULL;

void rt_managed_register(char *data_ptr) {
    if (data_ptr == NULL) return;
    MireManagedStringNode *node = (MireManagedStringNode *)malloc(sizeof(MireManagedStringNode));
    if (node == NULL) return;
    node->data_ptr = data_ptr;
    node->next = managed_strings;
    managed_strings = node;
}

void rt_managed_unregister(char *data_ptr) {
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
    for (MireManagedStringNode *node = managed_strings; node != NULL; node = node->next) {
        if (node->data_ptr == data_ptr) return 1;
    }
    return 0;
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
        free(value);
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

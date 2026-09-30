#include "runtime.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/*
 * Minimal ownership model.
 *
 * A runtime-created string is one libc allocation containing only its bytes
 * and trailing NUL. The compiler owns the returned value and emits exactly
 * one rt_managed_free call when that owner goes out of scope. Borrowed Mire
 * literals are never passed to rt_managed_free; functions that acquire an
 * owned value copy borrowed input through rt_managed_from_slice.
 *
 * There is deliberately no arena, header, reference counter, registry, or
 * replacement allocator in this tier. This mirrors Rust's ownership model:
 * ownership is enforced by the compiler, while allocation and deallocation
 * are direct libc calls.
 *
 * To protect against freeing static literals (which can happen when the
 * compiler's ownership analysis is conservative), we maintain a simple
 * allocation registry of pointers returned by our allocators.
 */

#define MINIMAL_REGISTRY_INITIAL_CAP 1024
static void **g_managed_allocations = NULL;
static size_t g_managed_alloc_count = 0;
static size_t g_managed_alloc_cap = 0;

static void minimal_registry_add(void *ptr) {
    if (!ptr) return;
    if (g_managed_alloc_count >= g_managed_alloc_cap) {
        size_t new_cap = g_managed_alloc_cap ? g_managed_alloc_cap * 2 : MINIMAL_REGISTRY_INITIAL_CAP;
        void **new_arr = (void **)realloc(g_managed_allocations, new_cap * sizeof(void *));
        if (!new_arr) return;
        g_managed_allocations = new_arr;
        g_managed_alloc_cap = new_cap;
    }
    g_managed_allocations[g_managed_alloc_count++] = ptr;
}

static bool minimal_registry_remove(void *ptr) {
    if (!ptr || !g_managed_allocations) return false;
    for (size_t i = 0; i < g_managed_alloc_count; i++) {
        if (g_managed_allocations[i] == ptr) {
            g_managed_allocations[i] = g_managed_allocations[--g_managed_alloc_count];
            return true;
        }
    }
    return false;
}

static bool minimal_registry_contains(void *ptr) {
    if (!ptr || !g_managed_allocations) return false;
    for (size_t i = 0; i < g_managed_alloc_count; i++) {
        if (g_managed_allocations[i] == ptr) return true;
    }
    return false;
}

size_t rt_string_growth_cap(size_t min_cap) {
    return min_cap;
}

char *rt_strdup_raw(const char *src) {
    if (!src) src = "";
    size_t len = strlen(src);
    char *out = (char *)malloc(len + 1);
    if (!out) return NULL;
    memcpy(out, src, len + 1);
    return out;
}

char *rt_strdup_raw_n(const char *src, size_t len) {
    char *out = (char *)malloc(len + 1);
    if (!out) return NULL;
    if (len) memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

char *rt_alloc_printf_raw_i64(const char *fmt, long long value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) return rt_strdup_raw("");
    char *out = (char *)malloc((size_t)needed + 1);
    if (!out) return rt_strdup_raw("");
    snprintf(out, (size_t)needed + 1, fmt, value);
    return out;
}

char *rt_managed_alloc(size_t len) {
    char *out = (char *)malloc(len + 1);
    if (!out) return NULL;
    out[len] = '\0';
    minimal_registry_add(out);
    return out;
}

char *rt_managed_from_slice(const char *src, size_t len) {
    if (!src) src = "";
    char *out = rt_managed_alloc(len);
    if (!out) return rt_strdup_raw("");
    if (len) memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

char *rt_managed_from_cstr(const char *src) {
    return rt_managed_from_slice(src ? src : "", src ? strlen(src) : 0);
}

char *rt_managed_ensure_managed(char *ptr) {
    /* The compiler has already established ownership for runtime returns. */
    return ptr ? ptr : rt_managed_from_slice("", 0);
}

char *rt_managed_printf_i64(const char *fmt, long long value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) return rt_managed_from_slice("", 0);
    char *out = rt_managed_alloc((size_t)needed);
    if (!out) return rt_managed_from_slice("", 0);
    snprintf(out, (size_t)needed + 1, fmt, value);
    return out;
}

char *rt_managed_printf_f64(const char *fmt, double value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) return rt_managed_from_slice("", 0);
    char *out = rt_managed_alloc((size_t)needed);
    if (!out) return rt_managed_from_slice("", 0);
    snprintf(out, (size_t)needed + 1, fmt, value);
    return out;
}

void rt_managed_free(char *value) {
    if (!value) return;
    if (!minimal_registry_contains(value)) {
        /* Not our allocation — likely a static literal or externally managed.
         * Do not free to avoid undefined behavior. */
        return;
    }
    minimal_registry_remove(value);
    free(value);
}

void rt_managed_cleanup_all(void) {}
int rt_managed_is_managed(const char *value) { (void)value; return 0; }
int rt_managed_contains(const char *value) { (void)value; return 0; }
void rt_managed_register(char *value) { (void)value; }
void rt_managed_unregister(char *value) { (void)value; }
void rt_managed_retain(char *value) { (void)value; }
MireManagedString *rt_string_header(const char *value) { (void)value; return NULL; }

int64_t rt_strings_len(const char *value) {
    return value ? (int64_t)strlen(value) : 0;
}

size_t rt_managed_len(const char *value) {
    return value ? strlen(value) : 0;
}

static size_t utf8_next(const unsigned char *s, size_t offset, size_t len) {
    unsigned char first = s[offset];
    size_t width = 1;
    if ((first & 0x80) == 0) return offset + 1;
    if ((first & 0xe0) == 0xc0) width = 2;
    else if ((first & 0xf0) == 0xe0) width = 3;
    else if ((first & 0xf8) == 0xf0) width = 4;
    else return offset + 1;
    if (offset + width > len) return offset + 1;
    for (size_t i = 1; i < width; i++) {
        if ((s[offset + i] & 0xc0) != 0x80) return offset + 1;
    }
    return offset + width;
}

int64_t rt_strings_len_utf8(const char *value) {
    if (!value) return 0;
    size_t len = strlen(value);
    size_t count = 0;
    for (size_t i = 0; i < len;) {
        i = utf8_next((const unsigned char *)value, i, len);
        count++;
    }
    return (int64_t)count;
}

char *rt_strings_substr_utf8(const char *input, int64_t start_cp, int64_t count_cp) {
    if (!input) return rt_managed_from_slice("", 0);
    if (start_cp < 0) start_cp = 0;
    size_t len = strlen(input);
    size_t start = 0;
    int64_t cp = 0;
    while (start < len && cp < start_cp) {
        start = utf8_next((const unsigned char *)input, start, len);
        cp++;
    }
    if (cp < start_cp) return rt_managed_from_slice("", 0);
    size_t end = len;
    if (count_cp > 0) {
        size_t cursor = start;
        int64_t remaining = count_cp;
        while (cursor < len && remaining > 0) {
            cursor = utf8_next((const unsigned char *)input, cursor, len);
            remaining--;
        }
        end = cursor;
    }
    return rt_managed_from_slice(input + start, end - start);
}

int64_t rt_strings_index_of_utf8(const char *value, const char *needle) {
    if (!value || !needle) return -1;
    const char *found = strstr(value, needle);
    if (!found) return -1;
    size_t prefix_len = (size_t)(found - value);
    size_t count = 0;
    for (size_t i = 0; i < prefix_len;) {
        i = utf8_next((const unsigned char *)value, i, prefix_len);
        count++;
    }
    return (int64_t)count;
}

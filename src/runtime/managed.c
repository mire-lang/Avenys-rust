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

            // Reinsert the following probe cluster. Leaving a hole would
            // make later entries unreachable to managed_ht_contains().
            size_t next = (idx + 1) % managed_ht_cap;
            while (managed_ht_keys[next]) {
                const char *cluster_key = managed_ht_keys[next];
                managed_ht_keys[next] = NULL;
                managed_ht_len--;
                managed_ht_put(cluster_key);
                next = (next + 1) % managed_ht_cap;
            }
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
    if (managed_ht_contains(data_ptr)) return;
    MireManagedStringNode *node = (MireManagedStringNode *)malloc(sizeof(MireManagedStringNode));
    if (node == NULL) return;
    if (!managed_ht_put(data_ptr)) {
        free(node);
        return;
    }
    node->data_ptr = data_ptr;
    node->next = managed_strings;
    managed_strings = node;
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

int rt_managed_is_managed(const char *data_ptr) {
    return data_ptr != NULL && managed_ht_contains(data_ptr);
}

// ── Header helpers ────────────────────────────────────────────────────

MireManagedString *rt_string_header(const char *data) {
    if (data == NULL) return NULL;
    return (MireManagedString *)((char *)data - offsetof(MireManagedString, data));
}

static size_t utf8_next(const unsigned char *s, size_t offset, size_t byte_len) {
    unsigned char first = s[offset];
    size_t width = 1;
    uint32_t codepoint = first;
    if ((first & 0x80) == 0) return offset + 1;
    if ((first & 0xe0) == 0xc0) {
        width = 2;
        codepoint = first & 0x1f;
    } else if ((first & 0xf0) == 0xe0) {
        width = 3;
        codepoint = first & 0x0f;
    } else if ((first & 0xf8) == 0xf0) {
        width = 4;
        codepoint = first & 0x07;
    } else {
        return offset + 1;
    }
    if (offset + width > byte_len) return offset + 1;
    for (size_t i = 1; i < width; i++) {
        unsigned char continuation = s[offset + i];
        if ((continuation & 0xc0) != 0x80) return offset + 1;
        codepoint = (codepoint << 6) | (continuation & 0x3f);
    }
    if ((width == 2 && codepoint < 0x80)
        || (width == 3 && codepoint < 0x800)
        || (width == 4 && codepoint < 0x10000)
        || codepoint > 0x10ffff
        || (codepoint >= 0xd800 && codepoint <= 0xdfff)) {
        return offset + 1;
    }
    return offset + width;
}

static size_t utf8_codepoint_count(const char *s, size_t byte_len) {
    size_t count = 0;
    for (size_t i = 0; i < byte_len;) {
        i = utf8_next((const unsigned char *)s, i, byte_len);
        count++;
    }
    return count;
}

// ── String growth ────────────────────────────────────────────────────

size_t rt_string_growth_cap(size_t min_cap) {
    size_t cap = 16;
    while (cap < min_cap) cap += cap >> 1;
    return cap;
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
    header->flags = MIRE_STR_MANAGED;
    header->utf8_cp = 0;
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
    if (!rt_managed_contains(value)) return;
    rt_managed_unregister(value);
    MireManagedString *header = rt_string_header(value);
    if (header) free(header);
}

void rt_managed_cleanup_all(void) {
    MireManagedStringNode *node = managed_strings;
    while (node != NULL) {
        MireManagedStringNode *next = node->next;
        MireManagedString *header = rt_string_header(node->data_ptr);
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

// ── Optimized len: uses cached header length instead of strlen ────────

int64_t rt_strings_len(const char *s) {
    if (s == NULL) return 0;
    if (rt_managed_is_managed(s)) {
        MireManagedString *hdr = rt_string_header(s);
        return (int64_t)hdr->len;
    }
    return (int64_t)strlen(s);
}

size_t rt_managed_len(const char *value) {
    if (value == NULL) return 0;
    if (rt_managed_is_managed(value)) {
        MireManagedString *header = rt_string_header(value);
        return header->len;
    }
    return strlen(value);
}

// ── UTF-8 codepoint length ────────────────────────────────────────────

int64_t rt_strings_len_utf8(const char *s) {
    if (s == NULL) return 0;
    size_t byte_len;
    MireManagedString *hdr = NULL;
    int managed = rt_managed_is_managed(s);
    if (managed) {
        hdr = rt_string_header(s);
        if (hdr->flags & MIRE_STR_UTF8_KNOWN) {
            return (int64_t)hdr->utf8_cp;
        }
        byte_len = hdr->len;
    } else {
        byte_len = strlen(s);
    }
    size_t cp = utf8_codepoint_count(s, byte_len);
    if (managed) {
        hdr->utf8_cp = (uint32_t)cp;
        hdr->flags |= MIRE_STR_UTF8_KNOWN;
    }
    return (int64_t)cp;
}

// ── UTF-8 substring by codepoint ──────────────────────────────────────

char *rt_strings_substr_utf8(const char *input, int64_t start_cp, int64_t count_cp) {
    if (!input) return rt_managed_from_slice("", 0);
    size_t byte_len;
    if (rt_managed_is_managed(input)) {
        MireManagedString *hdr = rt_string_header(input);
        byte_len = hdr->len;
    } else {
        byte_len = strlen(input);
    }
    if (start_cp < 0) start_cp = 0;

    size_t byte_start = 0;
    int64_t cp = 0;
    while (byte_start < byte_len && cp < start_cp) {
        byte_start = utf8_next((const unsigned char *)input, byte_start, byte_len);
        cp++;
    }
    if (cp < start_cp) return rt_managed_from_slice("", 0);

    size_t byte_end = byte_len;
    if (count_cp > 0) {
        int64_t remaining = count_cp;
        size_t i = byte_start;
        while (i < byte_len && remaining > 0) {
            i = utf8_next((const unsigned char *)input, i, byte_len);
            remaining--;
            if (remaining == 0) { byte_end = i; break; }
        }
        if (remaining > 0) byte_end = byte_len;
    }

    if (byte_end <= byte_start) return rt_managed_from_slice("", 0);
    return rt_managed_from_slice(input + byte_start, byte_end - byte_start);
}

// ── UTF-8 index_of ────────────────────────────────────────────────────

int64_t rt_strings_index_of_utf8(const char *s, const char *sub) {
    if (!s || !sub) return -1;
    if (*sub == '\0') return 0;
    const char *pos = strstr(s, sub);
    if (!pos) return -1;
    // Count codepoints from start to pos
    size_t byte_offset = (size_t)(pos - s);
    int64_t cp_count = 0;
    for (size_t i = 0; i < byte_offset;) {
        i = utf8_next((const unsigned char *)s, i, byte_offset);
        cp_count++;
    }
    return cp_count;
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

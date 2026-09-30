#include "runtime.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>

// ── Arena allocator with refcounting and size-class free lists ────────────────
//
// All managed strings are allocated from a single contiguous arena.
// Each string has a reference count in its header. When the refcount
// reaches zero, the string is added to a size-class free list for reuse.
// The arena is allocated at full size upfront via mmap(MAP_NORESERVE) to avoid
// pointer invalidation on realloc (which would invalidate free list pointers).
// The entire arena is released at program exit via rt_managed_cleanup_all().

#define ARENA_MAX_SIZE     (1024 * 1024 * 1024) // 1 GB cap
#define NUM_SIZE_CLASSES 9

// Size classes: 16, 32, 64, 128, 256, 512, 1024, 2048, 4096 bytes
static const size_t SIZE_CLASSES[NUM_SIZE_CLASSES] = {
    16, 32, 64, 128, 256, 512, 1024, 2048, 4096
};

typedef struct FreeBlock {
    size_t size;      // total block size (including header)
    char *next;       // next free block in same size class
} FreeBlock;

static char *arena_base = NULL;
static size_t arena_pos = 0;    // bytes used
static size_t arena_cap = 0;    // bytes allocated (always ARENA_MAX_SIZE after init)
static char *free_lists[NUM_SIZE_CLASSES] = {NULL};

static int arena_ensure_initialized(void) {
    if (arena_base) return 1;
    // Use mmap with MAP_NORESERVE to reserve virtual address space without committing physical memory
    arena_base = (char *)mmap(NULL, ARENA_MAX_SIZE, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
    if (arena_base == MAP_FAILED) return 0;
    arena_cap = ARENA_MAX_SIZE;
    arena_pos = 0;
    return 1;
}

// ── Size class helpers ────────────────────────────────────────────────────

static inline int size_class_index(size_t total_size) {
    for (int i = 0; i < NUM_SIZE_CLASSES; i++) {
        if (total_size <= SIZE_CLASSES[i]) return i;
    }
    return NUM_SIZE_CLASSES - 1; // largest class for oversized allocations
}

// ── Free list management ────────────────────────────────────────────────

static inline void free_list_add(char *header_ptr, size_t total_size) {
    int idx = size_class_index(total_size);
    MireManagedString *hdr = (MireManagedString *)header_ptr;
    char *data_area = (char *)(hdr + 1);  // data area starts after header
    FreeBlock *block = (FreeBlock *)data_area;
    block->size = total_size;
    block->next = free_lists[size_class_index(total_size)];
    free_lists[size_class_index(total_size)] = header_ptr;  // free list stores header pointers
}

static char *free_list_find(size_t need) {
    int idx = size_class_index(need);
    char **prev = (char **)&free_lists[idx];
    while (*prev) {
        char *header_ptr = *prev;
        MireManagedString *hdr = (MireManagedString *)header_ptr;
        char *data_area = (char *)(hdr + 1);
        FreeBlock *curr = (FreeBlock *)data_area;
        if (curr->size >= need) {
            *prev = curr->next;
            return header_ptr;
        }
        prev = &curr->next;
    }
    return NULL;
}

// ── Managed pointer detection ─────────────────────────────────────────

int rt_managed_is_managed(const char *data_ptr) {
    if (!data_ptr || !arena_base) return 0;
    return (size_t)(data_ptr - arena_base) < arena_pos;
}

int rt_managed_contains(const char *data_ptr) {
    return rt_managed_is_managed(data_ptr);
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

// ── String helpers (raw malloc, not arena) ────────────────────────────

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

// ── Arena allocation with size-class free lists ────────────────────────

char *rt_managed_alloc(size_t len) {
    size_t cap = rt_string_growth_cap(len);
    size_t total = sizeof(MireManagedString) + cap + 1;
    // Align to 16 bytes for performance
    total = (total + 15) & ~(size_t)15;

    // Try to find a free block first
    char *ptr = free_list_find(total);
    if (ptr) {
        MireManagedString *header = (MireManagedString *)ptr;
        header->len = len;
        header->cap = cap;
        header->flags = MIRE_STR_MANAGED;
        header->utf8_cp = 0;
        header->refs = 1;
        header->data[len] = '\0';
        return header->data;
    }

    // Allocate from arena (ensure initialized first)
    if (!arena_ensure_initialized()) return NULL;
    size_t total_size = sizeof(MireManagedString) + cap + 1;
    total = (total + 15) & ~(size_t)15;
    if (arena_pos + total > arena_cap) return NULL;
    MireManagedString *header = (MireManagedString *)(arena_base + arena_pos);
    arena_pos += total;
    header->len = len;
    header->cap = cap;
    header->flags = MIRE_STR_MANAGED;
    header->utf8_cp = 0;
    header->refs = 1;
    header->data[len] = '\0';
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

// ── Refcounting ────────────────────────────────────────────────────────

void rt_managed_retain(char *data_ptr) {
    if (!data_ptr) return;
    if (!rt_managed_is_managed(data_ptr)) return;
    MireManagedString *hdr = rt_string_header(data_ptr);
    if (hdr->refs > 0) {
        hdr->refs++;
    }
}

void rt_managed_free(char *value) {
    if (!value) return;
    if (!rt_managed_is_managed(value)) return;
    MireManagedString *hdr = rt_string_header(value);
    if (hdr->refs <= 0) return;
    hdr->refs--;
    if (hdr->refs == 0) {
        // Add to free list for reuse
        MireManagedString *hdr = rt_string_header(value);
        size_t total = sizeof(MireManagedString) + hdr->cap + 1;
        total = (total + 15) & ~(size_t)15;
        free_list_add(value - offsetof(MireManagedString, data), total);
    }
}

void rt_managed_cleanup_all(void) {
    if (arena_base) {
        munmap(arena_base, ARENA_MAX_SIZE);
        arena_base = NULL;
    }
    arena_pos = 0;
    arena_cap = 0;
    for (int i = 0; i < NUM_SIZE_CLASSES; i++) {
        free_lists[i] = NULL;
    }
}

// ── Register / unregister stubs (ABI compat, no-ops) ──────────────────

void rt_managed_register(char *data_ptr) {
    (void)data_ptr;
}

void rt_managed_unregister(char *data_ptr) {
    (void)data_ptr;
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
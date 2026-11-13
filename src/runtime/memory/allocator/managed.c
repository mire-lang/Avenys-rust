#include "runtime.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "pal.h"

// ── Arena allocator with refcounting and size-class free lists ────────────────
//
// All managed strings are allocated from a single contiguous arena.
// Each string has a reference count in its header. When the refcount
// reaches zero, the string is added to a size-class free list for reuse.
// The arena reserves its full address range through the PAL to avoid pointer
// invalidation on growth (which would invalidate free-list pointers). Hosts
// without lazy virtual-memory reservation use a different runtime tier.
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

// ── Memory statistics ────────────────────────────────────────────────────
static int64_t mem_alloc_count = 0;
static int64_t mem_free_count = 0;
static int64_t mem_alloc_bytes = 0;
static int64_t mem_free_bytes = 0;
static int64_t mem_peak_bytes = 0;
static int64_t mem_current_bytes = 0;
static int mem_stats_printed = 0;

static int arena_ensure_initialized(void) {
    if (arena_base) return 1;
    arena_base = (char *)pal_memory_reserve((int64_t)ARENA_MAX_SIZE);
    if (!arena_base) return 0;
    arena_cap = ARENA_MAX_SIZE;
    arena_pos = 0;
    return 1;
}

// ── Size class helpers ────────────────────────────────────────────────────

// SIZE_CLASSES are 16 * 2^i for i=0..8, so the class index is the ceiling of
// log2(total / 16), clamped to 8. This used to be `63 - __builtin_clzll(...)`,
// which tcc lowers to a real call to a helper (it has no clz instruction
// lowering), and that call showed up in allocation profiles. Three compares
// reach the same index and stay inline.
static inline int size_class_index(size_t total_size) {
    if (total_size <= 32) return total_size <= 16 ? 0 : 1;
    if (total_size <= 128) return total_size <= 64 ? 2 : 3;
    if (total_size <= 512) return total_size <= 256 ? 4 : 5;
    if (total_size <= 1024) return 6;
    return total_size <= 2048 ? 7 : 8;
}

// ── Free list management ────────────────────────────────────────────────

static inline void free_list_add(char *header_ptr, size_t total_size) {
    int idx = size_class_index(total_size);
    MireManagedString *hdr = (MireManagedString *)header_ptr;
    char *data_area = (char *)(hdr + 1);  // data area starts after header
    FreeBlock *block = (FreeBlock *)data_area;
    block->size = total_size;
    block->next = free_lists[idx];
    free_lists[idx] = header_ptr;  // free list stores header pointers
}

// Fast path: pop from free list without size check (caller ensures correct size class)
static inline char *free_list_pop_fast(int idx) {
    char *header_ptr = free_lists[idx];
    if (header_ptr) {
        MireManagedString *hdr = (MireManagedString *)header_ptr;
        char *data_area = (char *)(hdr + 1);
        FreeBlock *block = (FreeBlock *)data_area;
        free_lists[idx] = block->next;
    }
    return header_ptr;
}

static char *free_list_find(size_t need) {
    int idx = size_class_index(need);
    // Fast path: if there's a block in the exact size class, use it without size check
    char *fast = free_list_pop_fast(idx);
    if (fast) return fast;
    // Slow path: search for a block that fits (for oversized allocations)
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

// The address has to be the START of a block, not merely somewhere inside the
// arena. A pointer into the middle of a string — what strstr() returns, or
// `pos + from_len` in rt_strings_replace — passes a range check but has no
// header in front of it, so rt_string_header() read unrelated bytes as a
// length. That bogus length (2^40 in practice) went straight into a memcpy
// size, which is how a perfectly ordinary `strings::replace::all(s ".mr" "")`
// produced an empty string and eventually a double free.
//
// The flags word is the discriminator: only rt_managed_alloc() ever sets
// MIRE_STR_MANAGED, and it sets it on the block header. The offset is checked
// against the arena first so a pointer near the base cannot read before it.
int rt_managed_is_managed(const char *data_ptr) {
    if (!data_ptr || !arena_base) return 0;
    size_t offset = (size_t)((const char *)data_ptr - arena_base);
    if (offset < offsetof(MireManagedString, data) || offset >= arena_pos) return 0;
    const MireManagedString *hdr = rt_string_header(data_ptr);
    return (hdr->flags & MIRE_STR_MANAGED) != 0;
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

// ── String helpers (PAL allocations, not managed-arena strings) ───────

size_t rt_string_growth_cap(size_t min_cap) {
    size_t cap = 16;
    while (cap < min_cap) cap += cap >> 1;
    return cap;
}

char *rt_strdup_raw(const char *src) {
    size_t len = strlen(src) + 1;
    if (len > (size_t)INT64_MAX) return NULL;
    char *out = (char *)pal_alloc((int64_t)len);
    if (out == NULL) return NULL;
    memcpy(out, src, len);
    return out;
}

char *rt_strdup_raw_n(const char *src, size_t len) {
    if (len >= (size_t)INT64_MAX) return NULL;
    char *out = (char *)pal_alloc((int64_t)(len + 1));
    if (out == NULL) return NULL;
    if (len > 0) memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

// ── Arena allocation with size-class free lists ────────────────────────

char *rt_managed_alloc(size_t len) {
    // This function is compiled by tcc, which does not inline the `static
    // inline` helpers below: every one of them became a real call on the
    // numeric_format path (two million round trips). The growth-cap loop,
    // size-class index and free-list pop are written out here so an
    // allocation stays a single call from the caller.
    size_t cap = 16;
    while (cap < len) cap += cap >> 1;
    size_t total = sizeof(MireManagedString) + cap + 1;
    total = (total + 15) & ~(size_t)15;

    int idx;
    if (total <= 32) {
        idx = total <= 16 ? 0 : 1;
    } else if (total <= 128) {
        idx = total <= 64 ? 2 : 3;
    } else if (total <= 512) {
        idx = total <= 256 ? 4 : 5;
    } else if (total <= 1024) {
        idx = 6;
    } else {
        idx = total <= 2048 ? 7 : 8;
    }

    // Size classes group a range of capacities, so the head block can be
    // smaller than this request. Keep undersized blocks in the list and take
    // the first one whose recorded capacity can hold the new string.
    char **free_link = &free_lists[idx];
    char *header_ptr = NULL;
    while (*free_link) {
        char *candidate_ptr = *free_link;
        MireManagedString *candidate = (MireManagedString *)candidate_ptr;
        FreeBlock *block = (FreeBlock *)(candidate_ptr + sizeof(MireManagedString));
        if (candidate->cap >= cap) {
            *free_link = block->next;
            header_ptr = candidate_ptr;
            break;
        }
        free_link = &block->next;
    }
    if (header_ptr) {
        MireManagedString *header = (MireManagedString *)header_ptr;
        // Preserve the physical block capacity: lowering it would put a larger
        // block into a smaller free list when the string is released.
        header->len = len;
        // flags is already MIRE_STR_MANAGED from previous use
        header->utf8_cp = 0;
        header->refs = 1;
        header->data[len] = '\0';
        // Track allocation from free list (reuse)
        mem_alloc_count++;
        mem_alloc_bytes += total;
        mem_current_bytes += total;
        if (mem_current_bytes > mem_peak_bytes) mem_peak_bytes = mem_current_bytes;
        return header->data;
    }

    if (!arena_base && !arena_ensure_initialized()) return NULL;
    if (arena_pos + total > arena_cap) return NULL;
    MireManagedString *header = (MireManagedString *)(arena_base + arena_pos);
    arena_pos += total;
    header->len = len;
    header->cap = cap;
    header->flags = MIRE_STR_MANAGED;
    header->utf8_cp = 0;
    header->refs = 1;
    header->data[len] = '\0';
    // Track new arena allocation
    mem_alloc_count++;
    mem_alloc_bytes += total;
    mem_current_bytes += total;
    if (mem_current_bytes > mem_peak_bytes) mem_peak_bytes = mem_current_bytes;

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
    char *out = (char *)pal_alloc((int64_t)needed + 1);
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
    // Flattened for the same reason as rt_managed_alloc: tcc turns every
    // `static inline` helper into a real call. A drop to zero refs is the
    // common case for the temporary strings numeric_format creates, so the
    // managed check, size class and free-list push are written out here.
    if (!value || !arena_base) return;
    size_t offset = (size_t)(value - arena_base);
    if (offset < offsetof(MireManagedString, data) || offset >= arena_pos) return;
    MireManagedString *hdr = (MireManagedString *)(value - offsetof(MireManagedString, data));
    if (!(hdr->flags & MIRE_STR_MANAGED)) return;
    if (hdr->refs <= 0) return;
    hdr->refs--;
    if (hdr->refs != 0) return;

    // Add to free list for reuse.
    size_t total = sizeof(MireManagedString) + hdr->cap + 1;
    total = (total + 15) & ~(size_t)15;
    int idx;
    if (total <= 32) {
        idx = total <= 16 ? 0 : 1;
    } else if (total <= 128) {
        idx = total <= 64 ? 2 : 3;
    } else if (total <= 512) {
        idx = total <= 256 ? 4 : 5;
    } else if (total <= 1024) {
        idx = 6;
    } else {
        idx = total <= 2048 ? 7 : 8;
    }
    FreeBlock *block = (FreeBlock *)(hdr + 1);
    block->size = total;
    block->next = free_lists[idx];
    free_lists[idx] = (char *)hdr;
    // Track deallocation
    mem_free_count++;
    mem_free_bytes += total;
    if (mem_current_bytes >= total) mem_current_bytes -= total;
}

void rt_managed_cleanup_all(void) {
    if (arena_base) {
        (void)pal_memory_release(arena_base, (int64_t)ARENA_MAX_SIZE);
        arena_base = NULL;
    }
    arena_pos = 0;
    arena_cap = 0;
    for (int i = 0; i < NUM_SIZE_CLASSES; i++) {
        free_lists[i] = NULL;
    }
}

// ── Memory statistics ────────────────────────────────────────────────────

int64_t rt_mem_alloc_count(void) { return mem_alloc_count; }
int64_t rt_mem_alloc_bytes(void) { return mem_alloc_bytes; }
int64_t rt_mem_free_count(void) { return mem_free_count; }
int64_t rt_mem_free_bytes(void) { return mem_free_bytes; }
int64_t rt_mem_current_bytes(void) { return mem_current_bytes; }
int64_t rt_mem_peak_bytes(void) { return mem_peak_bytes; }
void rt_mem_reset_stats(void) {
    mem_alloc_count = 0;
    mem_alloc_bytes = 0;
    mem_free_count = 0;
    mem_free_bytes = 0;
    mem_current_bytes = 0;
    mem_peak_bytes = 0;
}

// Print memory statistics to stderr (called via codegen at program exit)
void rt_print_mem_stats(void) {
    fprintf(stderr, "MEM: alloc_count=%lld\n", (long long)mem_alloc_count);
    fprintf(stderr, "MEM: alloc_bytes=%lld\n", (long long)mem_alloc_bytes);
    fprintf(stderr, "MEM: free_count=%lld\n", (long long)mem_free_count);
    fprintf(stderr, "MEM: free_bytes=%lld\n", (long long)mem_free_bytes);
    fprintf(stderr, "MEM: current_bytes=%lld\n", (long long)mem_current_bytes);
    fprintf(stderr, "MEM: peak_bytes=%lld\n", (long long)mem_peak_bytes);
    fflush(stderr);
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
    rt_runtime_write_panic_message(message, 0);
    pal_process_exit(101);
}

#include <ctype.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

// Fast list implementation - inline storage
// Format: [capacity, length, data...]
// This avoids pointer arithmetic and extra allocations

static inline void *mire_list_create(int64_t initial_cap, int64_t elem_size) {
    if (initial_cap < 4) initial_cap = 4;
    int64_t *ptr = (int64_t *)malloc(16 + initial_cap * elem_size);
    if (!ptr) return NULL;
    ptr[0] = initial_cap;
    ptr[1] = 0;
    return ptr + 2;
}

static inline int64_t mire_list_len(void *list_ptr) {
    if (!list_ptr) return 0;
    return ((int64_t *)list_ptr)[-1];
}

static inline int64_t mire_list_cap(void *list_ptr) {
    if (!list_ptr) return 0;
    return ((int64_t *)list_ptr)[-2];
}

static inline void *mire_list_grow(void *list_ptr, int64_t elem_size) {
    int64_t old_cap = mire_list_cap(list_ptr);
    int64_t old_len = mire_list_len(list_ptr);
    int64_t new_cap = old_cap < 4 ? 4 : old_cap + (old_cap >> 1);  // 1.5x growth
    
    int64_t *old_ptr = ((int64_t *)list_ptr) - 2;
    int64_t *new_ptr = (int64_t *)realloc(old_ptr, 16 + new_cap * elem_size);
    if (!new_ptr) return list_ptr;
    
    new_ptr[0] = new_cap;
    return new_ptr + 2;
}

void *mire_list_push_i64(void *list_ptr, int64_t value) {
    if (!list_ptr) {
        list_ptr = mire_list_create(4, 8);
        if (!list_ptr) return NULL;
    }
    
    int64_t len = mire_list_len(list_ptr);
    int64_t cap = mire_list_cap(list_ptr);
    
    if (len >= cap) {
        list_ptr = mire_list_grow(list_ptr, 8);
    }
    
    ((int64_t *)list_ptr)[len] = value;
    ((int64_t *)list_ptr)[-1] = len + 1;
    return list_ptr;
}

void *mire_list_push_scalar(void *list_ptr, int64_t value, int64_t elem_size) {
    if (!list_ptr) {
        list_ptr = mire_list_create(4, elem_size > 0 ? elem_size : 8);
        if (!list_ptr) return NULL;
    }
    
    int64_t len = mire_list_len(list_ptr);
    int64_t cap = mire_list_cap(list_ptr);
    
    if (len >= cap) {
        list_ptr = mire_list_grow(list_ptr, elem_size > 0 ? elem_size : 8);
    }
    
    if (elem_size == 8) {
        ((int64_t *)list_ptr)[len] = value;
    } else if (elem_size == 4) {
        ((int32_t *)list_ptr)[len] = (int32_t)value;
    } else if (elem_size == 2) {
        ((int16_t *)list_ptr)[len] = (int16_t)value;
    } else if (elem_size == 1) {
        ((int8_t *)list_ptr)[len] = (int8_t)value;
    } else {
        memcpy((char *)list_ptr + len * elem_size, &value, elem_size);
    }
    
    ((int64_t *)list_ptr)[-1] = len + 1;
    return list_ptr;
}

typedef struct MireDictEntry {
    int64_t hash;
    int64_t next;
    int64_t key_i64;
    char *key_str;
} MireDictEntry;

typedef struct {
    int64_t len;
    int64_t cap;
    int64_t key_kind;
    int64_t value_kind;
    int64_t key_size;
    int64_t value_size;
    int64_t bucket_cap;
    int64_t *buckets;
    MireDictEntry *entries;
    uint8_t *key_storage;
    uint8_t *value_storage;
} MireDict;

enum {
    MIRE_KIND_SCALAR = 1,
    MIRE_KIND_BOOL = 2,
    MIRE_KIND_STR = 3,
    MIRE_KIND_MAP = 4,
    MIRE_KIND_PTR = 5,
};

char *mire_dict_to_string(void *dict_ptr);
void *mire_list_push_scalar(void *list_ptr, int64_t value, int64_t elem_size);

typedef struct {
    size_t len;
    size_t cap;
    char data[];
} MireManagedString;

static size_t mire_string_growth_cap(size_t min_cap) {
    size_t cap = 16;
    while (cap < min_cap) {
        cap += cap >> 1;
    }
    return cap;
}

static MireManagedString *mire_string_header(char *value) {
    if (value == NULL) {
        return NULL;
    }
    return (MireManagedString *)((char *)value - offsetof(MireManagedString, data));
}

static char *mire_strdup_raw(const char *src) {
    size_t len = strlen(src) + 1;
    char *out = (char *)malloc(len);
    if (out == NULL) {
        return NULL;
    }
    memcpy(out, src, len);
    return out;
}

static char *mire_managed_alloc(size_t len) {
    size_t cap = mire_string_growth_cap(len);
    MireManagedString *header =
        (MireManagedString *)malloc(sizeof(MireManagedString) + cap + 1);
    if (header == NULL) {
        return NULL;
    }
    header->len = len;
    header->cap = cap;
    header->data[len] = '\0';
    return header->data;
}

static char *mire_managed_from_slice(const char *src, size_t len) {
    char *out = mire_managed_alloc(len);
    if (out == NULL) {
        return mire_strdup_raw("");
    }
    if (len > 0) {
        memcpy(out, src, len);
    }
    out[len] = '\0';
    return out;
}

static char *mire_managed_printf_i64(const char *fmt, long long value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) {
        return mire_managed_from_slice("", 0);
    }
    char *out = mire_managed_alloc((size_t)needed);
    if (out == NULL) {
        return mire_managed_from_slice("", 0);
    }
    snprintf(out, (size_t)needed + 1, fmt, value);
    return out;
}

static char *mire_managed_printf_f64(const char *fmt, double value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) {
        return mire_managed_from_slice("", 0);
    }
    char *out = mire_managed_alloc((size_t)needed);
    if (out == NULL) {
        return mire_managed_from_slice("", 0);
    }
    snprintf(out, (size_t)needed + 1, fmt, value);
    return out;
}

static char *mire_alloc_printf_raw_i64(const char *fmt, long long value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) {
        return mire_strdup_raw("");
    }
    char *out = (char *)malloc((size_t)needed + 1);
    if (out == NULL) {
        return mire_strdup_raw("");
    }
    snprintf(out, (size_t)needed + 1, fmt, value);
    return out;
}

static int64_t mire_clock_ns(clockid_t clock_id) {
    struct timespec ts;
    if (clock_gettime(clock_id, &ts) != 0) {
        return 0;
    }
    return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
}

void mire_runtime_panic(const char *message) {
    if (message && *message) {
        fprintf(stderr, "runtime error: %s\n", message);
    } else {
        fprintf(stderr, "runtime error\n");
    }
    fflush(stderr);
    exit(101);
}

static double mire_cpu_mhz(void) {
    static int initialized = 0;
    static double cached = 0.0;

    if (initialized) {
        return cached;
    }
    initialized = 1;

    FILE *fh = fopen("/proc/cpuinfo", "r");
    if (fh == NULL) {
        return 0.0;
    }

    char line[256];
    while (fgets(line, sizeof(line), fh) != NULL) {
        for (char *p = line; *p != '\0'; ++p) {
            *p = (char)tolower((unsigned char)*p);
        }
        if (strncmp(line, "cpu mhz", 7) == 0) {
            char *colon = strchr(line, ':');
            if (colon != NULL) {
                cached = strtod(colon + 1, NULL);
            }
            break;
        }
    }

    fclose(fh);
    return cached;
}

static uint64_t mire_hash_string(const char *src) {
    uint64_t hash = 1469598103934665603ULL;
    if (src == NULL) {
        return hash;
    }
    while (*src != '\0') {
        hash ^= (uint64_t)(unsigned char)*src;
        hash *= 1099511628211ULL;
        ++src;
    }
    return hash;
}

static uint64_t mire_hash_u64(uint64_t value) {
    value ^= value >> 33;
    value *= 0xff51afd7ed558ccdULL;
    value ^= value >> 33;
    value *= 0xc4ceb9fe1a85ec53ULL;
    value ^= value >> 33;
    return value;
}

static uint64_t mire_hash_key(int64_t key_kind, int64_t key_i64, const void *key_ptr) {
    if (key_kind == MIRE_KIND_STR) {
        return mire_hash_string((const char *)key_ptr);
    }
    if (key_kind == MIRE_KIND_MAP || key_kind == MIRE_KIND_PTR) {
        return mire_hash_u64((uint64_t)(uintptr_t)key_ptr);
    }
    return mire_hash_u64((uint64_t)key_i64);
}

static int64_t mire_kind_size(int64_t kind) {
    switch (kind) {
        case MIRE_KIND_BOOL:
            return 1;
        case MIRE_KIND_STR:
        case MIRE_KIND_MAP:
        case MIRE_KIND_PTR:
            return 8;
        default:
            return 8;
    }
}

static void *mire_dict_key_slot(MireDict *dict, int64_t index) {
    return dict->key_storage + index * dict->key_size;
}

static void *mire_dict_value_slot(MireDict *dict, int64_t index) {
    return dict->value_storage + index * dict->value_size;
}

static void mire_dict_write_scalar(void *slot, int64_t size, int64_t value) {
    switch (size) {
        case 1:
            *(uint8_t *)slot = (uint8_t)value;
            break;
        case 2:
            *(uint16_t *)slot = (uint16_t)value;
            break;
        case 4:
            *(uint32_t *)slot = (uint32_t)value;
            break;
        default:
            *(int64_t *)slot = value;
            break;
    }
}

static int mire_dict_key_equals(
    const MireDict *dict,
    int64_t entry_index,
    int64_t key_i64,
    const void *key_ptr
) {
    const void *slot = dict->key_storage + entry_index * dict->key_size;
    if (dict->key_kind == MIRE_KIND_STR) {
        const char *stored = *(const char **)slot;
        return strcmp(stored, (const char *)key_ptr) == 0;
    }
    if (dict->key_kind == MIRE_KIND_MAP || dict->key_kind == MIRE_KIND_PTR) {
        const void *stored = *(const void **)slot;
        return stored == key_ptr;
    }
    int64_t stored = 0;
    int64_t size = dict->key_size;
    switch (size) {
        case 1:
            stored = *(const uint8_t *)slot;
            break;
        case 2:
            stored = *(const uint16_t *)slot;
            break;
        case 4:
            stored = *(const uint32_t *)slot;
            break;
        default:
            stored = *(const int64_t *)slot;
            break;
    }
    return stored == key_i64;
}

static void mire_dict_store_key(
    MireDict *dict,
    int64_t entry_index,
    int64_t key_i64,
    const void *key_ptr,
    int replacing
) {
    void *slot = mire_dict_key_slot(dict, entry_index);
    if (dict->key_kind == MIRE_KIND_STR) {
        if (replacing) {
            char *existing = *(char **)slot;
            if (existing != NULL) {
                free(existing);
            }
        }
        const char *src = (const char *)key_ptr;
        char *copy = mire_strdup_raw(src);
        memcpy(slot, &copy, sizeof(char *));
        return;
    }
    if (dict->key_kind == MIRE_KIND_MAP || dict->key_kind == MIRE_KIND_PTR) {
        void *ptr = (void *)key_ptr;
        memcpy(slot, &ptr, sizeof(void *));
        return;
    }
    mire_dict_write_scalar(slot, dict->key_size, key_i64);
}

static void mire_dict_store_value(
    MireDict *dict,
    int64_t entry_index,
    int64_t value_i64,
    const void *value_ptr
) {
    void *slot = mire_dict_value_slot(dict, entry_index);
    if (dict->value_kind == MIRE_KIND_STR) {
        void *ptr = (void *)value_ptr;
        memcpy(slot, &ptr, sizeof(void *));
        return;
    }
    if (dict->value_kind == MIRE_KIND_MAP || dict->value_kind == MIRE_KIND_PTR) {
        void *ptr = (void *)value_ptr;
        memcpy(slot, &ptr, sizeof(void *));
        return;
    }
    mire_dict_write_scalar(slot, dict->value_size, value_i64);
}

static int64_t mire_dict_read_scalar(const MireDict *dict, int64_t entry_index) {
    const void *slot = dict->value_storage + entry_index * dict->value_size;
    switch (dict->value_size) {
        case 1:
            return *(const uint8_t *)slot;
        case 2:
            return *(const uint16_t *)slot;
        case 4:
            return *(const uint32_t *)slot;
        default:
            return *(const int64_t *)slot;
    }
}

static int64_t mire_dict_read_key_scalar(const MireDict *dict, int64_t entry_index) {
    const void *slot = dict->key_storage + entry_index * dict->key_size;
    switch (dict->key_size) {
        case 1:
            return *(const uint8_t *)slot;
        case 2:
            return *(const uint16_t *)slot;
        case 4:
            return *(const uint32_t *)slot;
        default:
            return *(const int64_t *)slot;
    }
}

static void *mire_dict_read_ptr(const MireDict *dict, int64_t entry_index) {
    const void *slot = dict->value_storage + entry_index * dict->value_size;
    return (void *)*(const void **)slot;
}

static void *mire_dict_read_key_ptr(const MireDict *dict, int64_t entry_index) {
    const void *slot = dict->key_storage + entry_index * dict->key_size;
    return (void *)*(const void **)slot;
}

static void mire_dict_clear_buckets(MireDict *dict) {
    if (dict == NULL || dict->buckets == NULL || dict->bucket_cap <= 0) {
        return;
    }
    for (int64_t i = 0; i < dict->bucket_cap; ++i) {
        dict->buckets[i] = -1;
    }
}

static int mire_dict_rehash(MireDict *dict, int64_t bucket_cap) {
    if (dict == NULL) {
        return 0;
    }
    if (bucket_cap < 16) {
        bucket_cap = 16;
    }
    int64_t *buckets = (int64_t *)malloc((size_t)bucket_cap * sizeof(int64_t));
    if (buckets == NULL) {
        return 0;
    }
    free(dict->buckets);
    dict->buckets = buckets;
    dict->bucket_cap = bucket_cap;
    mire_dict_clear_buckets(dict);

    for (int64_t i = 0; i < dict->len; ++i) {
        int64_t bucket = (int64_t)(dict->entries[i].hash & (uint64_t)(dict->bucket_cap - 1));
        dict->entries[i].next = dict->buckets[bucket];
        dict->buckets[bucket] = i;
    }
    return 1;
}

static int mire_dict_resize_storage(MireDict *dict, int64_t new_cap) {
    if (dict == NULL) {
        return 0;
    }
    uint8_t *next_keys = (uint8_t *)realloc(
        dict->key_storage,
        (size_t)new_cap * (size_t)dict->key_size
    );
    if (next_keys == NULL) {
        return 0;
    }
    dict->key_storage = next_keys;
    uint8_t *next_values = (uint8_t *)realloc(
        dict->value_storage,
        (size_t)new_cap * (size_t)dict->value_size
    );
    if (next_values == NULL) {
        return 0;
    }
    dict->value_storage = next_values;
    return 1;
}

static int mire_dict_grow_entries(MireDict *dict) {
    if (dict == NULL) {
        return 0;
    }
    int64_t next_cap = dict->cap == 0 ? 4 : dict->cap * 2;
    MireDictEntry *next_entries = (MireDictEntry *)realloc(
        dict->entries,
        (size_t)next_cap * sizeof(MireDictEntry)
    );
    if (next_entries == NULL) {
        return 0;
    }
    dict->entries = next_entries;
    dict->cap = next_cap;
    if (!mire_dict_resize_storage(dict, next_cap)) {
        return 0;
    }
    return 1;
}

static int mire_dict_maybe_grow_buckets(MireDict *dict) {
    if (dict == NULL) {
        return 0;
    }
    if (dict->bucket_cap == 0) {
        return mire_dict_rehash(dict, 16);
    }
    if ((dict->len + 1) * 2 < dict->bucket_cap) {
        return 1;
    }
    return mire_dict_rehash(dict, dict->bucket_cap * 2);
}

static MireDict *mire_dict_ensure(void *dict_ptr) {
    MireDict *dict = (MireDict *)dict_ptr;
    if (dict != NULL) {
        return dict;
    }

    dict = (MireDict *)calloc(1, sizeof(MireDict));
    if (dict == NULL) {
        return NULL;
    }
    dict->cap = 4;
    dict->key_kind = MIRE_KIND_SCALAR;
    dict->value_kind = MIRE_KIND_SCALAR;
    dict->key_size = 8;
    dict->value_size = 8;
    dict->entries = (MireDictEntry *)calloc((size_t)dict->cap, sizeof(MireDictEntry));
    if (dict->entries == NULL) {
        free(dict);
        return NULL;
    }
    dict->key_storage = (uint8_t *)calloc((size_t)dict->cap, (size_t)dict->key_size);
    dict->value_storage = (uint8_t *)calloc((size_t)dict->cap, (size_t)dict->value_size);
    if (dict->key_storage == NULL || dict->value_storage == NULL) {
        free(dict->key_storage);
        free(dict->value_storage);
        free(dict->entries);
        free(dict);
        return NULL;
    }
    if (!mire_dict_rehash(dict, 16)) {
        free(dict->entries);
        free(dict->key_storage);
        free(dict->value_storage);
        free(dict);
        return NULL;
    }
    return dict;
}

static MireDict *mire_dict_ensure_kind(void *dict_ptr, int64_t key_kind, int64_t value_kind) {
    MireDict *dict = mire_dict_ensure(dict_ptr);
    if (dict == NULL) {
        return NULL;
    }
    if (dict->key_kind == MIRE_KIND_SCALAR) {
        dict->key_kind = key_kind;
    }
    if (dict->value_kind == MIRE_KIND_SCALAR) {
        dict->value_kind = value_kind;
    }
    dict->key_size = mire_kind_size(dict->key_kind);
    dict->value_size = mire_kind_size(dict->value_kind);
    mire_dict_resize_storage(dict, dict->cap);
    return dict;
}

static int64_t mire_dict_find(
    MireDict *dict,
    int64_t key_i64,
    const void *key_ptr,
    uint64_t hash
) {
    if (dict == NULL || dict->buckets == NULL || dict->bucket_cap <= 0) {
        return -1;
    }
    int64_t bucket = (int64_t)(hash & (uint64_t)(dict->bucket_cap - 1));
    int64_t index = dict->buckets[bucket];
    while (index >= 0) {
        MireDictEntry *entry = &dict->entries[index];
        if (entry->hash == hash && mire_dict_key_equals(dict, index, key_i64, key_ptr)) {
            return index;
        }
        index = entry->next;
    }
    return -1;
}

int64_t mire_wall_mark_ns(void) {
    return mire_clock_ns(CLOCK_MONOTONIC);
}

int64_t mire_wall_elapsed_ms(int64_t start_ns) {
    int64_t end_ns = mire_clock_ns(CLOCK_MONOTONIC);
    if (end_ns <= start_ns) {
        return 0;
    }
    return (end_ns - start_ns) / 1000000LL;
}

char *mire_wall_elapsed_ms_str(int64_t start_ns) {
    int64_t end_ns = mire_clock_ns(CLOCK_MONOTONIC);
    if (end_ns <= start_ns) {
        return mire_managed_from_slice("0.000", 5);
    }
    return mire_managed_printf_f64("%.3f", (double)(end_ns - start_ns) / 1000000.0);
}

int64_t mire_cpu_mark_ns(void) {
    return mire_clock_ns(CLOCK_PROCESS_CPUTIME_ID);
}

int64_t mire_cpu_elapsed_ms(int64_t start_ns) {
    int64_t end_ns = mire_clock_ns(CLOCK_PROCESS_CPUTIME_ID);
    if (end_ns <= start_ns) {
        return 0;
    }
    return (end_ns - start_ns) / 1000000LL;
}

char *mire_cpu_elapsed_ms_str(int64_t start_ns) {
    int64_t end_ns = mire_clock_ns(CLOCK_PROCESS_CPUTIME_ID);
    if (end_ns <= start_ns) {
        return mire_managed_from_slice("0.000", 5);
    }
    return mire_managed_printf_f64("%.3f", (double)(end_ns - start_ns) / 1000000.0);
}

int64_t mire_cpu_cycles_est(int64_t start_ns) {
    int64_t end_ns = mire_clock_ns(CLOCK_PROCESS_CPUTIME_ID);
    if (end_ns <= start_ns) {
        return 0;
    }
    double mhz = mire_cpu_mhz();
    if (mhz <= 0.0) {
        return 0;
    }
    double elapsed_ns = (double)(end_ns - start_ns);
    return (int64_t)(elapsed_ns * mhz / 1000.0);
}

int64_t mire_mem_process_bytes(void) {
    FILE *fh = fopen("/proc/self/status", "r");
    if (fh == NULL) {
        return 0;
    }

    char line[256];
    while (fgets(line, sizeof(line), fh) != NULL) {
        if (strncmp(line, "VmRSS:", 6) == 0) {
            long long kb = atoll(line + 6);
            fclose(fh);
            return (int64_t)kb * 1024LL;
        }
    }

    fclose(fh);
    return 0;
}

char *mire_mem_format(int64_t bytes) {
    return mire_managed_printf_i64("%lld B", (long long)bytes);
}

char *mire_gpu_snapshot(void) {
    return mire_managed_from_slice("available=false", 15);
}

char *mire_i64_to_string(int64_t value) {
    return mire_managed_printf_i64("%lld", (long long)value);
}

char *mire_bool_to_string(int64_t value) {
    return mire_managed_from_slice(value ? "true" : "false", value ? 4 : 5);
}

char *mire_string_copy(const char *value) {
    if (value == NULL) {
        return mire_managed_from_slice("", 0);
    }
    return mire_managed_from_slice(value, strlen(value));
}

char *mire_string_concat(const char *left, const char *right) {
    if (left == NULL) left = "";
    if (right == NULL) right = "";

    size_t left_len = strlen(left);
    size_t right_len = strlen(right);
    size_t total_len = left_len + right_len;
    char *result = mire_managed_alloc(total_len);
    if (result == NULL) {
        return mire_managed_from_slice("", 0);
    }
    if (left_len > 0) {
        memcpy(result, left, left_len);
    }
    if (right_len > 0) {
        memcpy(result + left_len, right, right_len);
    }
    result[total_len] = '\0';
    return result;
}

char *mire_string_append_owned(char *value, const char *suffix) {
    if (suffix == NULL || *suffix == '\0') {
        return value;
    }
    if (value == NULL) {
        return mire_string_copy(suffix);
    }

    MireManagedString *header = mire_string_header(value);
    size_t suffix_len = strlen(suffix);
    size_t new_len = header->len + suffix_len;
    if (new_len > header->cap) {
        size_t new_cap = mire_string_growth_cap(new_len);
        header = (MireManagedString *)realloc(
            header,
            sizeof(MireManagedString) + new_cap + 1
        );
        if (header == NULL) {
            return mire_managed_from_slice("", 0);
        }
        header->cap = new_cap;
        value = header->data;
    }

    memcpy(value + header->len, suffix, suffix_len);
    header->len = new_len;
    value[new_len] = '\0';
    return value;
}

char *mire_string_to_upper(const char *value) {
    if (value == NULL) {
        return mire_managed_from_slice("", 0);
    }
    size_t len = strlen(value);
    char *result = mire_managed_alloc(len);
    if (result == NULL) {
        return mire_managed_from_slice("", 0);
    }
    for (size_t i = 0; i < len; i++) {
        result[i] = (value[i] >= 'a' && value[i] <= 'z') ? (value[i] - 32) : value[i];
    }
    result[len] = '\0';
    return result;
}

char *mire_string_to_lower(const char *value) {
    if (value == NULL) {
        return mire_managed_from_slice("", 0);
    }
    size_t len = strlen(value);
    char *result = mire_managed_alloc(len);
    if (result == NULL) {
        return mire_managed_from_slice("", 0);
    }
    for (size_t i = 0; i < len; i++) {
        result[i] = (value[i] >= 'A' && value[i] <= 'Z') ? (value[i] + 32) : value[i];
    }
    result[len] = '\0';
    return result;
}

void mire_string_free(char *value) {
    if (value != NULL) {
        free(mire_string_header(value));
    }
}

void *mire_list_push_ptr(void *list_ptr, void *value) {
    if (!list_ptr) {
        list_ptr = mire_list_create(4, sizeof(void *));
        if (!list_ptr) return NULL;
    }
    
    int64_t len = mire_list_len(list_ptr);
    int64_t cap = mire_list_cap(list_ptr);
    
    if (len >= cap) {
        list_ptr = mire_list_grow(list_ptr, sizeof(void *));
    }
    
    ((void **)list_ptr)[len] = value;
    ((int64_t *)list_ptr)[-1] = len + 1;
    return list_ptr;
}

static char *mire_dict_format_scalar(int64_t value, int64_t kind) {
    if (kind == MIRE_KIND_BOOL) {
        return mire_strdup_raw(value ? "true" : "false");
    }
    return mire_alloc_printf_raw_i64("%lld", (long long)value);
}

static char *mire_dict_format_key(const MireDict *dict, int64_t entry_index) {
    int64_t kind = dict->key_kind;
    if (kind == MIRE_KIND_STR) {
        const char *src = *(const char **)(dict->key_storage + entry_index * dict->key_size);
        size_t len = strlen(src);
        char *out = (char *)malloc(len + 3);
        if (out == NULL) {
            return mire_strdup_raw("''");
        }
        out[0] = '\'';
        memcpy(out + 1, src, len);
        out[len + 1] = '\'';
        out[len + 2] = '\0';
        return out;
    }
    if (kind == MIRE_KIND_MAP || kind == MIRE_KIND_PTR) {
        return mire_strdup_raw("<ptr>");
    }
    int64_t scalar = mire_dict_read_key_scalar(dict, entry_index);
    return mire_dict_format_scalar(scalar, kind);
}

static char *mire_dict_format_value(const MireDict *dict, int64_t entry_index) {
    int64_t kind = dict->value_kind;
    if (kind == MIRE_KIND_STR) {
        const char *src = *(const char **)(dict->value_storage + entry_index * dict->value_size);
        size_t len = strlen(src);
        char *out = (char *)malloc(len + 3);
        if (out == NULL) {
            return mire_strdup_raw("''");
        }
        out[0] = '\'';
        memcpy(out + 1, src, len);
        out[len + 1] = '\'';
        out[len + 2] = '\0';
        return out;
    }
    if (kind == MIRE_KIND_MAP) {
        return mire_strdup_raw(mire_dict_to_string(mire_dict_read_ptr(dict, entry_index)));
    }
    if (kind == MIRE_KIND_PTR) {
        return mire_strdup_raw("<ptr>");
    }
    int64_t scalar = mire_dict_read_scalar(dict, entry_index);
    return mire_dict_format_scalar(scalar, kind);
}

int64_t mire_dict_get_i64(
    void *dict_ptr,
    int64_t key_kind,
    int64_t key_i64,
    void *key_ptr,
    int64_t default_value
) {
    MireDict *dict = (MireDict *)dict_ptr;
    uint64_t hash = mire_hash_key(key_kind, key_i64, key_ptr);
    int64_t entry_index = mire_dict_find(dict, key_i64, key_ptr, hash);
    if (entry_index < 0) {
        return default_value;
    }
    return mire_dict_read_scalar(dict, entry_index);
}

void *mire_dict_set_i64(
    void *dict_ptr,
    int64_t key_kind,
    int64_t value_kind,
    int64_t key_i64,
    void *key_ptr,
    int64_t value
) {
    MireDict *dict = mire_dict_ensure_kind(dict_ptr, key_kind, value_kind);
    if (dict == NULL) {
        return dict_ptr;
    }

    uint64_t hash = mire_hash_key(key_kind, key_i64, key_ptr);
    int64_t existing_index = mire_dict_find(dict, key_i64, key_ptr, hash);
    if (existing_index >= 0) {
        mire_dict_store_key(dict, existing_index, key_i64, key_ptr, 1);
        mire_dict_store_value(dict, existing_index, value, NULL);
        return dict;
    }

    if (dict->len == dict->cap && !mire_dict_grow_entries(dict)) {
        return dict;
    }
    if (!mire_dict_maybe_grow_buckets(dict)) {
        return dict;
    }

    int64_t index = dict->len;
    int64_t bucket = (int64_t)(hash & (uint64_t)(dict->bucket_cap - 1));
    dict->entries[index].hash = hash;
    dict->entries[index].next = dict->buckets[bucket];
    dict->buckets[bucket] = index;
    mire_dict_store_key(dict, index, key_i64, key_ptr, 0);
    mire_dict_store_value(dict, index, value, NULL);
    dict->len += 1;
    return dict;
}

void *mire_dict_get_ptr(
    void *dict_ptr,
    int64_t key_kind,
    int64_t key_i64,
    void *key_ptr,
    void *default_value
) {
    MireDict *dict = (MireDict *)dict_ptr;
    uint64_t hash = mire_hash_key(key_kind, key_i64, key_ptr);
    int64_t entry_index = mire_dict_find(dict, key_i64, key_ptr, hash);
    if (entry_index < 0) {
        return default_value;
    }
    return mire_dict_read_ptr(dict, entry_index);
}

void *mire_dict_set_ptr(
    void *dict_ptr,
    int64_t key_kind,
    int64_t value_kind,
    int64_t key_i64,
    void *key_ptr,
    void *value
) {
    MireDict *dict = mire_dict_ensure_kind(dict_ptr, key_kind, value_kind);
    if (dict == NULL) {
        return dict_ptr;
    }

    uint64_t hash = mire_hash_key(key_kind, key_i64, key_ptr);
    int64_t existing_index = mire_dict_find(dict, key_i64, key_ptr, hash);
    if (existing_index >= 0) {
        mire_dict_store_key(dict, existing_index, key_i64, key_ptr, 1);
        mire_dict_store_value(dict, existing_index, 0, value);
        return dict;
    }

    if (dict->len == dict->cap && !mire_dict_grow_entries(dict)) {
        return dict;
    }
    if (!mire_dict_maybe_grow_buckets(dict)) {
        return dict;
    }

    int64_t index = dict->len;
    int64_t bucket = (int64_t)(hash & (uint64_t)(dict->bucket_cap - 1));
    dict->entries[index].hash = hash;
    dict->entries[index].next = dict->buckets[bucket];
    dict->buckets[bucket] = index;
    mire_dict_store_key(dict, index, key_i64, key_ptr, 0);
    mire_dict_store_value(dict, index, 0, value);
    dict->len += 1;
    return dict;
}

char *mire_dict_to_string(void *dict_ptr) {
    MireDict *dict = (MireDict *)dict_ptr;
    if (dict == NULL || dict->len == 0) {
        return mire_managed_from_slice("{}", 2);
    }

    size_t total = 3;
    for (int64_t i = 0; i < dict->len; ++i) {
        char *key_repr = mire_dict_format_key(dict, i);
        char *value_repr = mire_dict_format_value(dict, i);
        total += strlen(key_repr) + strlen(value_repr) + 4;
        free(key_repr);
        free(value_repr);
    }

    char *out = mire_managed_alloc(total - 1);
    if (out == NULL) {
        return mire_managed_from_slice("{}", 2);
    }

    size_t pos = 0;
    out[pos++] = '{';
    for (int64_t i = 0; i < dict->len; ++i) {
        char *key_repr = mire_dict_format_key(dict, i);
        char *value_repr = mire_dict_format_value(dict, i);
        if (i > 0) {
            out[pos++] = ',';
            out[pos++] = ' ';
        }
        size_t key_len = strlen(key_repr);
        memcpy(out + pos, key_repr, key_len);
        pos += key_len;
        out[pos++] = ':';
        out[pos++] = ' ';
        size_t value_len = strlen(value_repr);
        memcpy(out + pos, value_repr, value_len);
        pos += value_len;
        free(key_repr);
        free(value_repr);
    }
    out[pos++] = '}';
    out[pos] = '\0';
    return out;
}

char *mire_strings_replace(const char *input, const char *from, const char *to) {
    if (input == NULL || from == NULL || to == NULL) {
        return mire_managed_from_slice("", 0);
    }

    size_t input_len = strlen(input);
    size_t from_len = strlen(from);
    size_t to_len = strlen(to);
    if (from_len == 0) {
        return mire_managed_from_slice(input, input_len);
    }

    size_t matches = 0;
    const char *cursor = input;
    while ((cursor = strstr(cursor, from)) != NULL) {
        matches += 1;
        cursor += from_len;
    }

    size_t out_len = input_len;
    if (to_len >= from_len) {
        out_len += matches * (to_len - from_len);
    } else {
        out_len -= matches * (from_len - to_len);
    }
    char *out = mire_managed_alloc(out_len);
    if (out == NULL) {
        return mire_managed_from_slice(input, input_len);
    }

    const char *src = input;
    char *dst = out;
    while ((cursor = strstr(src, from)) != NULL) {
        size_t chunk = (size_t)(cursor - src);
        memcpy(dst, src, chunk);
        dst += chunk;
        memcpy(dst, to, to_len);
        dst += to_len;
        src = cursor + from_len;
    }

    strcpy(dst, src);
    return out;
}

void *mire_list_concat(void *left_ptr, void *right_ptr) {
    if (!left_ptr && !right_ptr) return NULL;
    
    int64_t left_len = mire_list_len(left_ptr);
    int64_t right_len = mire_list_len(right_ptr);
    int64_t total_len = left_len + right_len;
    
    if (total_len == 0) return NULL;
    
    int64_t new_cap = 4;
    while (new_cap < total_len) new_cap += new_cap >> 1;
    
    int64_t *new_base = (int64_t *)malloc(16 + new_cap * 8);
    if (!new_base) return NULL;
    
    new_base[0] = new_cap;
    new_base[1] = total_len;
    int64_t *new_data = new_base + 2;
    
    if (left_ptr && left_len > 0) {
        memcpy(new_data, left_ptr, (size_t)left_len * 8);
    }
    
    if (right_ptr && right_len > 0) {
        memcpy(new_data + left_len, right_ptr, (size_t)right_len * 8);
    }
    
    return new_data;
}

void *mire_list_slice(void *list_ptr, int64_t start, int64_t end) {
    if (!list_ptr) return NULL;
    
    int64_t len = mire_list_len(list_ptr);
    if (start < 0) start = 0;
    if (end > len) end = len;
    if (start >= end) return NULL;
    
    int64_t new_len = end - start;
    int64_t new_cap = 4;
    while (new_cap < new_len) new_cap += new_cap >> 1;
    
    int64_t *new_base = (int64_t *)malloc(16 + new_cap * 8);
    if (!new_base) return NULL;
    
    new_base[0] = new_cap;
    new_base[1] = new_len;
    int64_t *new_data = new_base + 2;
    
    memcpy(new_data, (int64_t *)list_ptr + start, (size_t)new_len * 8);
    
    return new_data;
}

char *mire_strings_split(const char *input, const char *delimiter) {
    if (input == NULL || delimiter == NULL) {
        return mire_managed_from_slice("", 0);
    }
    
    size_t delim_len = strlen(delimiter);
    if (delim_len == 0) {
        return mire_managed_from_slice(input, strlen(input));
    }
    
    size_t input_len = strlen(input);
    
    size_t count = 1;
    const char *p = input;
    while ((p = strstr(p, delimiter)) != NULL) {
        count++;
        p += delim_len;
    }
    
    char **parts = (char **)malloc(count * sizeof(char *));
    if (parts == NULL) {
        return mire_managed_from_slice("", 0);
    }
    
    char *input_copy = mire_strdup_raw(input);
    char *token = strtok(input_copy, delimiter);
    size_t idx = 0;
    
    while (token != NULL && idx < count) {
        parts[idx++] = mire_strdup_raw(token);
        token = strtok(NULL, delimiter);
    }
    
    free(input_copy);
    
    size_t total_len = 0;
    for (size_t i = 0; i < idx; i++) {
        total_len += strlen(parts[i]) + 1;
    }
    
    char *result = mire_managed_alloc(total_len);
    if (result == NULL) {
        for (size_t i = 0; i < idx; i++) {
            free(parts[i]);
        }
        free(parts);
        return mire_managed_from_slice("", 0);
    }
    
    result[0] = '\0';
    for (size_t i = 0; i < idx; i++) {
        if (i > 0) {
            strcat(result, " ");
        }
        strcat(result, parts[i]);
    }
    
    for (size_t i = 0; i < idx; i++) {
        free(parts[i]);
    }
    free(parts);
    
    return result;
}

char *mire_strings_join(char **parts, size_t count, const char *delimiter) {
    if (parts == NULL || count == 0) {
        return mire_managed_from_slice("", 0);
    }
    
    if (delimiter == NULL) {
        delimiter = "";
    }
    size_t delim_len = strlen(delimiter);
    
    size_t total_len = 0;
    for (size_t i = 0; i < count; i++) {
        if (parts[i] != NULL) {
            total_len += strlen(parts[i]);
        }
    }
    
    if (count > 1 && delim_len > 0) {
        total_len += (count - 1) * delim_len;
    }
    
    char *result = mire_managed_alloc(total_len);
    if (result == NULL) {
        return mire_managed_from_slice("", 0);
    }
    
    result[0] = '\0';
    for (size_t i = 0; i < count; i++) {
        if (i > 0 && delim_len > 0) {
            strcat(result, delimiter);
        }
        if (parts[i] != NULL) {
            strcat(result, parts[i]);
        }
    }
    
    return result;
}

char *mire_strings_trim(const char *input) {
    if (input == NULL) {
        return mire_managed_from_slice("", 0);
    }
    
    const char *start = input;
    const char *end = input + strlen(input);
    
    while (*start == ' ' || *start == '\t' || *start == '\n' || *start == '\r') {
        start++;
    }
    
    while (end > start && (*(end - 1) == ' ' || *(end - 1) == '\t' || *(end - 1) == '\n' || *(end - 1) == '\r')) {
        end--;
    }
    
    size_t len = end - start;
    char *result = mire_managed_alloc(len);
    if (result == NULL) {
        return mire_managed_from_slice("", 0);
    }
    
    memcpy(result, start, len);
    result[len] = '\0';
    
    return result;
}

void *mire_dict_keys(void *dict_ptr) {
    MireDict *dict = (MireDict *)dict_ptr;
    if (dict == NULL || dict->len == 0) {
        return NULL;
    }
    
    int64_t new_cap = 4;
    while (new_cap < dict->len) new_cap += new_cap >> 1;
    
    int64_t *result = (int64_t *)malloc(16 + new_cap * 8);
    if (result == NULL) return NULL;
    
    result[0] = new_cap;
    result[1] = dict->len;
    int64_t *data = result + 2;
    
    for (int64_t i = 0; i < dict->len; i++) {
        if (dict->key_kind == MIRE_KIND_SCALAR) {
            data[i] = dict->entries[i].key_i64;
        } else {
            data[i] = (int64_t)dict->entries[i].key_str;
        }
    }
    
    return data;
}

void *mire_dict_values(void *dict_ptr) {
    MireDict *dict = (MireDict *)dict_ptr;
    if (dict == NULL || dict->len == 0) {
        return NULL;
    }
    
    int64_t new_cap = 4;
    while (new_cap < dict->len) new_cap += new_cap >> 1;
    
    int64_t *result = (int64_t *)malloc(16 + new_cap * 8);
    if (result == NULL) return NULL;
    
    result[0] = new_cap;
    result[1] = dict->len;
    int64_t *data = result + 2;
    
    for (int64_t i = 0; i < dict->len; i++) {
        if (dict->value_kind == MIRE_KIND_PTR) {
            data[i] = (int64_t)(dict->value_storage + i * dict->value_size);
        } else {
            data[i] = *(int64_t *)(dict->value_storage + i * dict->value_size);
        }
    }
    
    return data;
}
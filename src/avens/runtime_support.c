#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

typedef struct {
    uint64_t hash;
    int64_t next;
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

static char *mire_strdup(const char *src) {
    size_t len = strlen(src) + 1;
    char *out = (char *)malloc(len);
    if (out == NULL) {
        return NULL;
    }
    memcpy(out, src, len);
    return out;
}

static char *mire_alloc_printf(const char *fmt, long long value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) {
        return mire_strdup("");
    }
    char *out = (char *)malloc((size_t)needed + 1);
    if (out == NULL) {
        return mire_strdup("");
    }
    snprintf(out, (size_t)needed + 1, fmt, value);
    return out;
}

static char *mire_alloc_printf_f64(const char *fmt, double value) {
    int needed = snprintf(NULL, 0, fmt, value);
    if (needed < 0) {
        return mire_strdup("");
    }
    char *out = (char *)malloc((size_t)needed + 1);
    if (out == NULL) {
        return mire_strdup("");
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
        char *copy = mire_strdup(src);
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
        return mire_strdup("0.000");
    }
    return mire_alloc_printf_f64("%.3f", (double)(end_ns - start_ns) / 1000000.0);
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
        return mire_strdup("0.000");
    }
    return mire_alloc_printf_f64("%.3f", (double)(end_ns - start_ns) / 1000000.0);
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
    return mire_alloc_printf("%lld B", (long long)bytes);
}

char *mire_gpu_snapshot(void) {
    return mire_strdup("available=false");
}

char *mire_i64_to_string(int64_t value) {
    return mire_alloc_printf("%lld", (long long)value);
}

char *mire_bool_to_string(int64_t value) {
    return mire_strdup(value ? "true" : "false");
}

char *mire_string_copy(const char *value) {
    if (value == NULL) {
        return mire_strdup("");
    }
    return mire_strdup(value);
}

char *mire_string_to_upper(const char *value) {
    if (value == NULL) {
        return mire_strdup("");
    }
    size_t len = strlen(value);
    char *result = (char *)malloc(len + 1);
    if (result == NULL) {
        return mire_strdup("");
    }
    for (size_t i = 0; i < len; i++) {
        result[i] = (value[i] >= 'a' && value[i] <= 'z') ? (value[i] - 32) : value[i];
    }
    result[len] = '\0';
    return result;
}

char *mire_string_to_lower(const char *value) {
    if (value == NULL) {
        return mire_strdup("");
    }
    size_t len = strlen(value);
    char *result = (char *)malloc(len + 1);
    if (result == NULL) {
        return mire_strdup("");
    }
    for (size_t i = 0; i < len; i++) {
        result[i] = (value[i] >= 'A' && value[i] <= 'Z') ? (value[i] + 32) : value[i];
    }
    result[len] = '\0';
    return result;
}

void mire_string_free(char *value) {
    if (value != NULL) {
        free(value);
    }
}

void *mire_list_push_i64(void *list_ptr, int64_t value) {
    return mire_list_push_scalar(list_ptr, value, 8);
}

void *mire_list_push_scalar(void *list_ptr, int64_t value, int64_t elem_size) {
    uint8_t *base = NULL;
    int64_t *len_ptr = (int64_t *)list_ptr;
    int64_t len = 0;
    int64_t cap = 0;

    if (elem_size <= 0) {
        elem_size = 8;
    }

    if (len_ptr == NULL) {
        cap = 4;
        base = (uint8_t *)malloc((size_t)(16 + cap * elem_size));
        if (base == NULL) {
            return NULL;
        }
        ((int64_t *)base)[0] = cap;
        ((int64_t *)base)[1] = 0;
        len_ptr = (int64_t *)(base + 8);
    } else {
        base = ((uint8_t *)list_ptr) - 8;
        cap = ((int64_t *)base)[0];
        len = len_ptr[0];
        if (cap <= 0) {
            cap = len > 0 ? len : 4;
        }
        if (len >= cap) {
            int64_t next_cap = cap < 4 ? 4 : cap * 2;
            uint8_t *next_base = (uint8_t *)realloc(base, (size_t)(16 + next_cap * elem_size));
            if (next_base == NULL) {
                return list_ptr;
            }
            base = next_base;
            ((int64_t *)base)[0] = next_cap;
            len_ptr = (int64_t *)(base + 8);
            cap = next_cap;
        }
    }

    len = len_ptr[0];
    uint8_t *slot = ((uint8_t *)len_ptr) + 8 + (size_t)(len * elem_size);
    switch (elem_size) {
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
    len_ptr[0] = len + 1;
    return len_ptr;
}

void *mire_list_push_ptr(void *list_ptr, void *value) {
    intptr_t *len_ptr = (intptr_t *)list_ptr;
    intptr_t *base = NULL;
    intptr_t len = 0;
    intptr_t cap = 0;

    if (len_ptr == NULL) {
        cap = 4;
        base = (intptr_t *)malloc((size_t)(2 + cap) * sizeof(intptr_t));
        if (base == NULL) {
            return NULL;
        }
        base[0] = cap;
        base[1] = 0;
        len_ptr = base + 1;
    } else {
        base = len_ptr - 1;
        cap = base[0];
        len = len_ptr[0];
        if (cap <= 0) {
            cap = len > 0 ? len : 4;
        }
        if (len >= cap) {
            intptr_t next_cap = cap < 4 ? 4 : cap * 2;
            intptr_t *next_base = (intptr_t *)realloc(
                base,
                (size_t)(2 + next_cap) * sizeof(intptr_t)
            );
            if (next_base == NULL) {
                return list_ptr;
            }
            base = next_base;
            base[0] = next_cap;
            len_ptr = base + 1;
        }
    }

    len = len_ptr[0];
    len_ptr[1 + len] = (intptr_t)value;
    len_ptr[0] = len + 1;
    return len_ptr;
}

static char *mire_dict_format_scalar(int64_t value, int64_t kind) {
    if (kind == MIRE_KIND_BOOL) {
        return mire_strdup(value ? "true" : "false");
    }
    return mire_alloc_printf("%lld", (long long)value);
}

static char *mire_dict_format_key(const MireDict *dict, int64_t entry_index) {
    int64_t kind = dict->key_kind;
    if (kind == MIRE_KIND_STR) {
        const char *src = *(const char **)(dict->key_storage + entry_index * dict->key_size);
        size_t len = strlen(src);
        char *out = (char *)malloc(len + 3);
        if (out == NULL) {
            return mire_strdup("''");
        }
        out[0] = '\'';
        memcpy(out + 1, src, len);
        out[len + 1] = '\'';
        out[len + 2] = '\0';
        return out;
    }
    if (kind == MIRE_KIND_MAP || kind == MIRE_KIND_PTR) {
        return mire_strdup("<ptr>");
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
            return mire_strdup("''");
        }
        out[0] = '\'';
        memcpy(out + 1, src, len);
        out[len + 1] = '\'';
        out[len + 2] = '\0';
        return out;
    }
    if (kind == MIRE_KIND_MAP) {
        return mire_dict_to_string(mire_dict_read_ptr(dict, entry_index));
    }
    if (kind == MIRE_KIND_PTR) {
        return mire_strdup("<ptr>");
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
        return mire_strdup("{}");
    }

    size_t total = 3;
    for (int64_t i = 0; i < dict->len; ++i) {
        char *key_repr = mire_dict_format_key(dict, i);
        char *value_repr = mire_dict_format_value(dict, i);
        total += strlen(key_repr) + strlen(value_repr) + 4;
        free(key_repr);
        free(value_repr);
    }

    char *out = (char *)malloc(total);
    if (out == NULL) {
        return mire_strdup("{}");
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
        return mire_strdup("");
    }

    size_t input_len = strlen(input);
    size_t from_len = strlen(from);
    size_t to_len = strlen(to);
    if (from_len == 0) {
        return mire_strdup(input);
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
    char *out = (char *)malloc(out_len + 1);
    if (out == NULL) {
        return mire_strdup(input);
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
    intptr_t *left_len_ptr = (intptr_t *)left_ptr;
    intptr_t *right_len_ptr = (intptr_t *)right_ptr;
    
    intptr_t left_len = left_len_ptr ? left_len_ptr[0] : 0;
    intptr_t right_len = right_len_ptr ? right_len_ptr[0] : 0;
    intptr_t total_len = left_len + right_len;
    
    if (total_len == 0) {
        return NULL;
    }
    
    intptr_t cap = 4;
    while (cap < total_len) {
        cap *= 2;
    }
    
    intptr_t *new_base = (intptr_t *)malloc((size_t)(2 + cap) * sizeof(intptr_t));
    if (new_base == NULL) {
        return NULL;
    }
    
    new_base[0] = cap;
    new_base[1] = total_len;
    intptr_t *new_len_ptr = new_base + 1;
    
    if (left_len_ptr && left_len > 0) {
        intptr_t *left_base = left_len_ptr - 1;
        intptr_t left_cap = left_base[0];
        memcpy(new_len_ptr, left_len_ptr, (size_t)left_len * sizeof(intptr_t));
    }
    
    if (right_len_ptr && right_len > 0) {
        intptr_t *right_base = right_len_ptr - 1;
        intptr_t right_cap = right_base[0];
        memcpy(new_len_ptr + left_len, right_len_ptr, (size_t)right_len * sizeof(intptr_t));
    }
    
    return new_len_ptr;
}

void *mire_list_slice(void *list_ptr, int64_t start, int64_t end) {
    intptr_t *len_ptr = (intptr_t *)list_ptr;
    if (len_ptr == NULL) {
        return NULL;
    }
    
    intptr_t len = len_ptr[0];
    if (start < 0) start = 0;
    if (end > len) end = len;
    if (start >= end) {
        return NULL;
    }
    
    intptr_t new_len = end - start;
    intptr_t cap = 4;
    while (cap < new_len) {
        cap *= 2;
    }
    
    intptr_t *new_base = (intptr_t *)malloc((size_t)(2 + cap) * sizeof(intptr_t));
    if (new_base == NULL) {
        return NULL;
    }
    
    new_base[0] = cap;
    new_base[1] = new_len;
    intptr_t *new_len_ptr = new_base + 1;
    
    memcpy(new_len_ptr, len_ptr + start, (size_t)new_len * sizeof(intptr_t));
    
    return new_len_ptr;
}
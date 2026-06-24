#include "runtime.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// ── Hash functions ───────────────────────────────────────────────────

static uint64_t hash_string(const char *src) {
    uint64_t hash = 1469598103934665603ULL;
    if (src == NULL) return hash;
    while (*src != '\0') {
        hash ^= (uint64_t)(unsigned char)*src;
        hash *= 1099511628211ULL;
        ++src;
    }
    return hash;
}

static uint64_t hash_u64(uint64_t value) {
    value ^= value >> 33;
    value *= 0xff51afd7ed558ccdULL;
    value ^= value >> 33;
    value *= 0xc4ceb9fe1a85ec53ULL;
    value ^= value >> 33;
    return value;
}

static uint64_t hash_key(int64_t key_kind, int64_t key_i64, const void *key_ptr) {
    if (key_kind == MIRE_KIND_STR) return hash_string((const char *)key_ptr);
    if (key_kind == MIRE_KIND_MAP || key_kind == MIRE_KIND_PTR)
        return hash_u64((uint64_t)(uintptr_t)key_ptr);
    return hash_u64((uint64_t)key_i64);
}

// ── Internal helpers ─────────────────────────────────────────────────

static int64_t kind_size(int64_t kind) {
    switch (kind) {
        case MIRE_KIND_BOOL: return 1;
        case MIRE_KIND_STR:
        case MIRE_KIND_MAP:
        case MIRE_KIND_PTR:  return 8;
        default:             return 8;
    }
}

static void *key_slot(MireDict *dict, int64_t index) {
    return dict->key_storage + index * dict->key_size;
}

static void *value_slot(MireDict *dict, int64_t index) {
    return dict->value_storage + index * dict->value_size;
}

static void write_scalar(void *slot, int64_t size, int64_t value) {
    switch (size) {
        case 1: *(uint8_t *)slot = (uint8_t)value; break;
        case 2: *(uint16_t *)slot = (uint16_t)value; break;
        case 4: *(uint32_t *)slot = (uint32_t)value; break;
        default: *(int64_t *)slot = value; break;
    }
}

static int key_equals(const MireDict *dict, int64_t entry_index,
                       int64_t key_i64, const void *key_ptr)
{
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
    switch (dict->key_size) {
        case 1: stored = *(const uint8_t *)slot; break;
        case 2: stored = *(const uint16_t *)slot; break;
        case 4: stored = *(const uint32_t *)slot; break;
        default: stored = *(const int64_t *)slot; break;
    }
    return stored == key_i64;
}

static void store_key(MireDict *dict, int64_t entry_index,
                       int64_t key_i64, const void *key_ptr, int replacing)
{
    void *slot = key_slot(dict, entry_index);
    if (dict->key_kind == MIRE_KIND_STR) {
        if (replacing) {
            char *existing = *(char **)slot;
            if (existing) free(existing);
        }
        char *copy = rt_strdup_raw((const char *)key_ptr);
        memcpy(slot, &copy, sizeof(char *));
        return;
    }
    if (dict->key_kind == MIRE_KIND_MAP || dict->key_kind == MIRE_KIND_PTR) {
        memcpy(slot, &key_ptr, sizeof(void *));
        return;
    }
    write_scalar(slot, dict->key_size, key_i64);
}

static void store_value(MireDict *dict, int64_t entry_index,
                         int64_t value_i64, const void *value_ptr,
                         int replacing)
{
    void *slot = value_slot(dict, entry_index);
    if (dict->value_kind == MIRE_KIND_STR) {
        if (replacing) {
            char *existing = *(char **)slot;
            if (existing) free(existing);
        }
        memcpy(slot, &value_ptr, sizeof(void *));
        return;
    }
    if (dict->value_kind == MIRE_KIND_MAP || dict->value_kind == MIRE_KIND_PTR) {
        memcpy(slot, &value_ptr, sizeof(void *));
        return;
    }
    write_scalar(slot, dict->value_size, value_i64);
}

static int64_t read_scalar(const MireDict *dict, int64_t entry_index) {
    const void *slot = value_slot((MireDict *)dict, entry_index);
    switch (dict->value_size) {
        case 1: return *(const uint8_t *)slot;
        case 2: return *(const uint16_t *)slot;
        case 4: return *(const uint32_t *)slot;
        default: return *(const int64_t *)slot;
    }
}

static int64_t read_key_scalar(const MireDict *dict, int64_t entry_index) {
    const void *slot = key_slot((MireDict *)dict, entry_index);
    switch (dict->key_size) {
        case 1: return *(const uint8_t *)slot;
        case 2: return *(const uint16_t *)slot;
        case 4: return *(const uint32_t *)slot;
        default: return *(const int64_t *)slot;
    }
}

static void *read_ptr(const MireDict *dict, int64_t entry_index) {
    return *(void **)value_slot((MireDict *)dict, entry_index);
}

static void *read_key_ptr(const MireDict *dict, int64_t entry_index) {
    return *(void **)key_slot((MireDict *)dict, entry_index);
}

static void clear_buckets(MireDict *dict) {
    for (int64_t i = 0; i < dict->bucket_cap; i++) dict->buckets[i] = -1;
    for (int64_t i = 0; i < dict->cap; i++) dict->entries[i].next = -1;
}

static int rehash(MireDict *dict, int64_t bucket_cap) {
    int64_t *new_buckets = (int64_t *)calloc((size_t)bucket_cap, sizeof(int64_t));
    if (!new_buckets) return 0;
    int64_t *old_buckets = dict->buckets;
    int64_t old_bucket_cap = dict->bucket_cap;
    dict->buckets = new_buckets;
    dict->bucket_cap = bucket_cap;
    for (int64_t i = 0; i < bucket_cap; i++) dict->buckets[i] = -1;
    for (int64_t i = 0; i < dict->len; i++) {
        int64_t h = dict->entries[i].hash;
        int64_t bi = h % bucket_cap;
        if (bi < 0) bi = -bi;
        dict->entries[i].next = dict->buckets[bi];
        dict->buckets[bi] = i;
    }
    if (old_buckets) free(old_buckets);
    return 1;
}

static int resize_storage(MireDict *dict, int64_t new_cap) {
    uint8_t *new_key = (uint8_t *)realloc(dict->key_storage, (size_t)new_cap * dict->key_size);
    uint8_t *new_val = (uint8_t *)realloc(dict->value_storage, (size_t)new_cap * dict->value_size);
    MireDictEntry *new_entries = (MireDictEntry *)realloc(dict->entries, (size_t)new_cap * sizeof(MireDictEntry));
    if (!new_key || !new_val || !new_entries) return 0;
    dict->key_storage = new_key;
    dict->value_storage = new_val;
    dict->entries = new_entries;
    dict->cap = new_cap;
    return 1;
}

static int grow_entries(MireDict *dict) {
    int64_t new_cap = dict->cap < 8 ? 8 : dict->cap * 2;
    if (!resize_storage(dict, new_cap)) return 0;
    return 1;
}

static int maybe_grow_buckets(MireDict *dict) {
    if (dict->bucket_cap <= 0 || dict->len > dict->bucket_cap * 2)
        return rehash(dict, dict->bucket_cap < 8 ? 8 : dict->bucket_cap * 2);
    return 1;
}

// ── Public API ───────────────────────────────────────────────────────

void *rt_dict_ensure(void *dict_ptr) {
    if (dict_ptr) return (MireDict *)dict_ptr;
    MireDict *dict = (MireDict *)calloc(1, sizeof(MireDict));
    if (!dict) return NULL;
    dict->bucket_cap = 8;
    dict->buckets = (int64_t *)calloc(8, sizeof(int64_t));
    if (!dict->buckets) { free(dict); return NULL; }
    for (int64_t i = 0; i < 8; i++) dict->buckets[i] = -1;
    return dict;
}

void *rt_dict_ensure_kind(void *dict_ptr, int64_t key_kind, int64_t value_kind) {
    MireDict *dict = (MireDict *)rt_dict_ensure(dict_ptr);
    if (!dict) return NULL;
    if (dict->cap == 0) {
        dict->key_kind = key_kind;
        dict->value_kind = value_kind;
        dict->key_size = kind_size(key_kind);
        dict->value_size = kind_size(value_kind);
        if (!resize_storage(dict, 8)) return dict;
    }
    return dict;
}

static int64_t dict_find(MireDict *dict, int64_t key_i64, const void *key_ptr, uint64_t hash) {
    if (!dict || dict->bucket_cap <= 0) return -1;
    int64_t bi = (int64_t)(hash % (uint64_t)dict->bucket_cap);
    if (bi < 0) bi = -bi;
    int64_t idx = dict->buckets[bi];
    while (idx >= 0) {
        if (dict->entries[idx].hash == (int64_t)hash && key_equals(dict, idx, key_i64, key_ptr))
            return idx;
        idx = dict->entries[idx].next;
    }
    return -1;
}

int64_t rt_dict_len(void *dict_ptr) {
    if (!dict_ptr) return 0;
    return ((MireDict *)dict_ptr)->len;
}

int64_t rt_dict_get_i64(void *dict_ptr, int64_t key_kind, int64_t key_i64,
                         void *key_ptr, int64_t default_value)
{
    if (!dict_ptr) return default_value;
    MireDict *dict = (MireDict *)dict_ptr;
    uint64_t h = hash_key(key_kind, key_i64, key_ptr);
    int64_t idx = dict_find(dict, key_i64, key_ptr, h);
    if (idx < 0) return default_value;
    return read_scalar(dict, idx);
}

void *rt_dict_set_i64(void *dict_ptr, int64_t key_kind, int64_t value_kind,
                       int64_t key_i64, void *key_ptr, int64_t value)
{
    MireDict *dict = (MireDict *)rt_dict_ensure_kind(dict_ptr, key_kind, value_kind);
    if (!dict) return dict_ptr;
    uint64_t h = hash_key(key_kind, key_i64, key_ptr);
    int64_t idx = dict_find(dict, key_i64, key_ptr, h);
    if (idx >= 0) {
        store_value(dict, idx, value, NULL, 1);
        return dict;
    }
    if (dict->len >= dict->cap) {
        if (!grow_entries(dict)) return dict;
    }
    idx = dict->len;
    dict->entries[idx].hash = (int64_t)h;
    dict->entries[idx].next = -1;
    store_key(dict, idx, key_i64, key_ptr, 0);
    store_value(dict, idx, value, NULL, 0);
    dict->len++;
    int64_t bi = (int64_t)(h % (uint64_t)dict->bucket_cap);
    if (bi < 0) bi = -bi;
    dict->entries[idx].next = dict->buckets[bi];
    dict->buckets[bi] = idx;
    maybe_grow_buckets(dict);
    return dict;
}

void *rt_dict_get_ptr(void *dict_ptr, int64_t key_kind, int64_t key_i64,
                       void *key_ptr, void *default_value)
{
    if (!dict_ptr) return default_value;
    MireDict *dict = (MireDict *)dict_ptr;
    uint64_t h = hash_key(key_kind, key_i64, key_ptr);
    int64_t idx = dict_find(dict, key_i64, key_ptr, h);
    if (idx < 0) return default_value;
    return read_ptr(dict, idx);
}

void *rt_dict_set_ptr(void *dict_ptr, int64_t key_kind, int64_t value_kind,
                       int64_t key_i64, void *key_ptr, void *value)
{
    MireDict *dict = (MireDict *)rt_dict_ensure_kind(dict_ptr, key_kind, value_kind);
    if (!dict) return dict_ptr;
    uint64_t h = hash_key(key_kind, key_i64, key_ptr);
    int64_t idx = dict_find(dict, key_i64, key_ptr, h);
    if (idx >= 0) {
        store_value(dict, idx, 0, value, 1);
        return dict;
    }
    if (dict->len >= dict->cap) {
        if (!grow_entries(dict)) return dict;
    }
    idx = dict->len;
    dict->entries[idx].hash = (int64_t)h;
    dict->entries[idx].next = -1;
    store_key(dict, idx, key_i64, key_ptr, 0);
    store_value(dict, idx, 0, value, 0);
    dict->len++;
    int64_t bi = (int64_t)(h % (uint64_t)dict->bucket_cap);
    if (bi < 0) bi = -bi;
    dict->entries[idx].next = dict->buckets[bi];
    dict->buckets[bi] = idx;
    maybe_grow_buckets(dict);
    return dict;
}

int64_t rt_dict_has(void *dict_ptr, int64_t key_kind, int64_t key_i64, void *key_ptr) {
    if (!dict_ptr) return 0;
    MireDict *dict = (MireDict *)dict_ptr;
    uint64_t h = hash_key(key_kind, key_i64, key_ptr);
    return dict_find(dict, key_i64, key_ptr, h) >= 0 ? 1 : 0;
}

void *rt_dict_remove(void *dict_ptr, int64_t key_kind, int64_t key_i64, void *key_ptr) {
    if (!dict_ptr) return dict_ptr;
    MireDict *dict = (MireDict *)dict_ptr;
    uint64_t h = hash_key(key_kind, key_i64, key_ptr);
    int64_t bi = (int64_t)(h % (uint64_t)dict->bucket_cap);
    if (bi < 0) bi = -bi;
    int64_t prev = -1;
    int64_t idx = dict->buckets[bi];
    while (idx >= 0) {
        if (dict->entries[idx].hash == (int64_t)h && key_equals(dict, idx, key_i64, key_ptr)) {
            if (prev < 0) dict->buckets[bi] = dict->entries[idx].next;
            else dict->entries[prev].next = dict->entries[idx].next;
            dict->len--;
            if (idx != dict->len) {
                if (dict->key_kind == MIRE_KIND_STR) {
                    char *old_key = *(char **)key_slot(dict, idx);
                    if (old_key) free(old_key);
                }
                if (dict->value_kind == MIRE_KIND_STR) {
                    char *old_val = *(char **)value_slot(dict, idx);
                    if (old_val) free(old_val);
                }
                dict->entries[idx] = dict->entries[dict->len];
                memcpy(key_slot(dict, idx), key_slot(dict, dict->len), (size_t)dict->key_size);
                memcpy(value_slot(dict, idx), value_slot(dict, dict->len), (size_t)dict->value_size);
                int64_t moved_bi = (int64_t)((uint64_t)dict->entries[idx].hash % (uint64_t)dict->bucket_cap);
                if (moved_bi < 0) moved_bi = -moved_bi;
                int64_t *cursor = &dict->buckets[moved_bi];
                while (*cursor >= 0 && *cursor != dict->len) cursor = &dict->entries[*cursor].next;
                if (*cursor == dict->len) *cursor = idx;
            }
            return dict;
        }
        prev = idx;
        idx = dict->entries[idx].next;
    }
    return dict;
}

// ── Formatting / to_string ───────────────────────────────────────────

static char *format_scalar(int64_t value, int64_t kind) {
    if (kind == MIRE_KIND_BOOL)
        return rt_strdup_raw(value ? "true" : "false");
    return rt_alloc_printf_raw_i64("%lld", (long long)value);
}

static char *format_key(const MireDict *dict, int64_t entry_index) {
    if (dict->key_kind == MIRE_KIND_STR) {
        const char *s = *(const char **)key_slot((MireDict *)dict, entry_index);
        size_t len = strlen(s);
        char *out = (char *)malloc(len + 3);
        if (!out) return rt_strdup_raw("");
        out[0] = '"';
        memcpy(out + 1, s, len);
        out[len + 1] = '"';
        out[len + 2] = '\0';
        return out;
    }
    return format_scalar(read_key_scalar(dict, entry_index), dict->key_kind);
}

static void free_repr(char *repr) {
    if (!repr) return;
    if (rt_managed_contains(repr)) rt_managed_free(repr);
    else free(repr);
}

char *rt_dict_to_string(void *dict_ptr) {
    if (!dict_ptr) return rt_managed_from_slice("{}", 2);
    MireDict *dict = (MireDict *)dict_ptr;
    if (dict->len == 0) return rt_managed_from_slice("{}", 2);
    size_t total = 2;
    for (int64_t i = 0; i < dict->len; i++) {
        char *k = format_key(dict, i);
        char *v = NULL;
        if (dict->value_kind == MIRE_KIND_STR) {
            const char *s = *(const char **)value_slot(dict, i);
            size_t slen = strlen(s);
            v = (char *)malloc(slen + 3);
            if (v) { v[0] = '"'; memcpy(v + 1, s, slen); v[slen + 1] = '"'; v[slen + 2] = '\0'; }
        } else if (dict->value_kind == MIRE_KIND_MAP) {
            void *sub = read_ptr(dict, i);
            v = rt_dict_to_string(sub);
        } else {
            v = format_scalar(read_scalar(dict, i), dict->value_kind);
        }
        total += (k ? strlen(k) : 4) + 2 + (v ? strlen(v) : 4);
        if (i < dict->len - 1) total += 2;
        free_repr(k);
        free_repr(v);
    }
    char *out = rt_managed_alloc(total);
    if (!out) return rt_managed_from_slice("{}", 2);
    size_t pos = 0;
    out[pos++] = '{';
    for (int64_t i = 0; i < dict->len; i++) {
        if (i > 0) { out[pos++] = ','; out[pos++] = ' '; }
        char *k = format_key(dict, i);
        char *v = NULL;
        if (dict->value_kind == MIRE_KIND_STR) {
            const char *s = *(const char **)value_slot(dict, i);
            size_t slen = strlen(s);
            v = (char *)malloc(slen + 3);
            if (v) { v[0] = '"'; memcpy(v + 1, s, slen); v[slen + 1] = '"'; v[slen + 2] = '\0'; }
        } else if (dict->value_kind == MIRE_KIND_MAP) {
            v = rt_dict_to_string(read_ptr(dict, i));
        } else {
            v = format_scalar(read_scalar(dict, i), dict->value_kind);
        }
        if (k) { size_t klen = strlen(k); memcpy(out + pos, k, klen); pos += klen; }
        out[pos++] = ':'; out[pos++] = ' ';
        if (v) { size_t vlen = strlen(v); memcpy(out + pos, v, vlen); pos += vlen; }
        free_repr(k);
        free_repr(v);
    }
    out[pos++] = '}';
    out[pos] = '\0';
    return out;
}

void *rt_dict_keys(void *dict_ptr) {
    if (!dict_ptr) return rt_list_create(4, 8);
    MireDict *dict = (MireDict *)dict_ptr;
    void *list = rt_list_create(dict->len < 4 ? 4 : dict->len, 8);
    for (int64_t i = 0; i < dict->len; i++) {
        void *k = read_key_ptr(dict, i);
        list = rt_list_push_ptr(list, k);
    }
    return list;
}

void *rt_dict_values(void *dict_ptr) {
    if (!dict_ptr) return rt_list_create(4, 8);
    MireDict *dict = (MireDict *)dict_ptr;
    void *list = rt_list_create(dict->len < 4 ? 4 : dict->len, 8);
    for (int64_t i = 0; i < dict->len; i++) {
        if (dict->value_kind == MIRE_KIND_STR || dict->value_kind == MIRE_KIND_MAP || dict->value_kind == MIRE_KIND_PTR) {
            void *v = read_ptr(dict, i);
            list = rt_list_push_ptr(list, v);
        } else {
            int64_t v = read_scalar(dict, i);
            list = rt_list_push_i64(list, v);
        }
    }
    return list;
}

int64_t rt_dicts_len(void *dict) { return rt_dict_len(dict); }
void *rt_dicts_get(void *dict, const char *key) {
    return rt_dict_get_ptr(dict, 3, 0, (void *)key, NULL);
}
void *rt_dicts_set(void *dict, const char *key, void *value) {
    return rt_dict_set_ptr(dict, 3, dict ? ((MireDict *)dict)->value_kind : 3, 0, (void *)key, value);
}
void *rt_dicts_set_with_kind(void *dict, const char *key, void *value, int64_t value_kind) {
    return rt_dict_set_ptr(dict, 3, value_kind, 0, (void *)key, value);
}
void *rt_dicts_set_i64(void *dict, const char *key, int64_t value) {
    return rt_dict_set_i64(dict, 3, 1, 0, (void *)key, value);
}
int64_t rt_dicts_has(void *dict, const char *key) {
    return rt_dict_has(dict, 3, 0, (void *)key);
}
void *rt_dicts_remove(void *dict, const char *key) {
    return rt_dict_remove(dict, 3, 0, (void *)key);
}
void *rt_dicts_keys(void *dict) { return rt_dict_keys(dict); }
void *rt_dicts_values(void *dict) { return rt_dict_values(dict); }
int64_t rt_dicts_entries(void *dict) { return rt_dict_len(dict); }
void *rt_dicts_merge(void *a, void *b) {
    MireDict *dict_b = (MireDict *)b;
    if (!dict_b) return a;
    for (int64_t i = 0; i < dict_b->len; i++) {
        const void *key_slot_ptr = dict_b->key_storage + i * dict_b->key_size;
        if (dict_b->key_kind == MIRE_KIND_STR) {
            const char *k = *(const char **)key_slot_ptr;
            const void *value_slot_ptr = dict_b->value_storage + i * dict_b->value_size;
            if (dict_b->value_kind == MIRE_KIND_STR || dict_b->value_kind == MIRE_KIND_MAP || dict_b->value_kind == MIRE_KIND_PTR) {
                void *v = *(void **)value_slot_ptr;
                a = rt_dict_set_ptr(a, MIRE_KIND_STR, dict_b->value_kind, 0, (void *)k, v);
            } else {
                int64_t v = *(int64_t *)value_slot_ptr;
                a = rt_dict_set_i64(a, MIRE_KIND_STR, MIRE_KIND_SCALAR, 0, (void *)k, v);
            }
        } else {
            int64_t k;
            switch (dict_b->key_size) {
                case 1: k = *(uint8_t *)key_slot_ptr; break;
                case 2: k = *(uint16_t *)key_slot_ptr; break;
                case 4: k = *(uint32_t *)key_slot_ptr; break;
                default: k = *(int64_t *)key_slot_ptr; break;
            }
            const void *value_slot_ptr = dict_b->value_storage + i * dict_b->value_size;
            if (dict_b->value_kind == MIRE_KIND_STR || dict_b->value_kind == MIRE_KIND_MAP || dict_b->value_kind == MIRE_KIND_PTR) {
                void *v = *(void **)value_slot_ptr;
                a = rt_dict_set_ptr(a, dict_b->key_kind, dict_b->value_kind, k, NULL, v);
            } else {
                int64_t v;
                switch (dict_b->value_size) {
                    case 1: v = *(uint8_t *)value_slot_ptr; break;
                    case 2: v = *(uint16_t *)value_slot_ptr; break;
                    case 4: v = *(uint32_t *)value_slot_ptr; break;
                    default: v = *(int64_t *)value_slot_ptr; break;
                }
                a = rt_dict_set_i64(a, dict_b->key_kind, MIRE_KIND_SCALAR, k, NULL, v);
            }
        }
    }
    return a;
}
int64_t rt_dicts_is_empty(void *dict) { return rt_dict_len(dict) <= 0 ? 1 : 0; }

void rt_dict_free(void *dict_ptr) {
    if (!dict_ptr) return;
    MireDict *dict = (MireDict *)dict_ptr;
    // Free all string keys (they are rt_strdup_raw'd copies)
    if (dict->key_kind == MIRE_KIND_STR) {
        for (int64_t i = 0; i < dict->len; i++) {
            char *key = *(char **)(dict->key_storage + i * dict->key_size);
            if (key) free(key);
        }
    }
    // Free all string values
    if (dict->value_kind == MIRE_KIND_STR) {
        for (int64_t i = 0; i < dict->len; i++) {
            char *val = *(char **)(dict->value_storage + i * dict->value_size);
            if (val) free(val);
        }
    }
    free(dict->buckets);
    free(dict->entries);
    free(dict->key_storage);
    free(dict->value_storage);
    free(dict);
}

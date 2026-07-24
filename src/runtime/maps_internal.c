#include "maps_internal.h"
#include <stdlib.h>
#include <string.h>

// ═══════════════════════════════════════════════════════════════════════
//  Hash map internals — hashing, slot access, bucket management
//
//  This file contains the internal implementation of the hash map data
//  structure. Public API lives in maps.c.
// ═══════════════════════════════════════════════════════════════════════

// ── Hash functions ───────────────────────────────────────────────────

uint64_t mire_hash_string(const char *src) {
    uint64_t hash = 1469598103934665603ULL;
    if (src == NULL) return hash;
    while (*src != '\0') {
        hash ^= (uint64_t)(unsigned char)*src;
        hash *= 1099511628211ULL;
        ++src;
    }
    return hash;
}

uint64_t mire_hash_u64(uint64_t value) {
    value ^= value >> 33;
    value *= 0xff51afd7ed558ccdULL;
    value ^= value >> 33;
    value *= 0xc4ceb9fe1a85ec53ULL;
    value ^= value >> 33;
    return value;
}

uint64_t mire_hash_key(int64_t key_kind, int64_t key_i64, const void *key_ptr) {
    if (key_kind == MIRE_KIND_STR) return mire_hash_string((const char *)key_ptr);
    if (key_kind == MIRE_KIND_MAP || key_kind == MIRE_KIND_PTR)
        return mire_hash_u64((uint64_t)(uintptr_t)key_ptr);
    return mire_hash_u64((uint64_t)key_i64);
}

// ── Slot access ──────────────────────────────────────────────────────

int64_t mire_kind_size(int64_t kind) {
    switch (kind) {
        case MIRE_KIND_BOOL: return 1;
        case MIRE_KIND_STR:
        case MIRE_KIND_MAP:
        case MIRE_KIND_PTR:  return 8;
        default:             return 8;
    }
}

void *mire_key_slot(MireDict *dict, int64_t index) {
    return dict->key_storage + index * dict->key_size;
}

void *mire_value_slot(MireDict *dict, int64_t index) {
    return dict->value_storage + index * dict->value_size;
}

void mire_write_scalar(void *slot, int64_t size, int64_t value) {
    switch (size) {
        case 1: *(uint8_t *)slot = (uint8_t)value; break;
        case 2: *(uint16_t *)slot = (uint16_t)value; break;
        case 4: *(uint32_t *)slot = (uint32_t)value; break;
        default: *(int64_t *)slot = value; break;
    }
}

int mire_key_equals(const MireDict *dict, int64_t entry_index,
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

void mire_store_key(MireDict *dict, int64_t entry_index,
                     int64_t key_i64, const void *key_ptr, int replacing)
{
    void *slot = mire_key_slot(dict, entry_index);
    if (dict->key_kind == MIRE_KIND_STR) {
        if (replacing) {
            char *existing = *(char **)slot;
            if (existing) {
                if (rt_managed_is_managed(existing)) rt_managed_free(existing);
                else free(existing);
            }
        }
        char *copy = rt_strdup_raw((const char *)key_ptr);
        memcpy(slot, &copy, sizeof(char *));
        return;
    }
    if (dict->key_kind == MIRE_KIND_MAP || dict->key_kind == MIRE_KIND_PTR) {
        memcpy(slot, &key_ptr, sizeof(void *));
        return;
    }
    mire_write_scalar(slot, dict->key_size, key_i64);
}

void mire_store_value(MireDict *dict, int64_t entry_index,
                       int64_t value_i64, const void *value_ptr, int replacing)
{
    void *slot = mire_value_slot(dict, entry_index);
    if (dict->value_kind == MIRE_KIND_STR) {
        if (replacing) {
            char *existing = *(char **)slot;
            if (existing) {
                if (rt_managed_is_managed(existing)) rt_managed_free(existing);
                else free(existing);
            }
        }
        char *copy = rt_strdup_raw((const char *)value_ptr);
        memcpy(slot, &copy, sizeof(void *));
        return;
    }
    if (dict->value_kind == MIRE_KIND_MAP || dict->value_kind == MIRE_KIND_PTR) {
        memcpy(slot, &value_ptr, sizeof(void *));
        return;
    }
    mire_write_scalar(slot, dict->value_size, value_i64);
}

int64_t mire_read_scalar(const MireDict *dict, int64_t entry_index) {
    const void *slot = mire_value_slot((MireDict *)dict, entry_index);
    switch (dict->value_size) {
        case 1: return *(const uint8_t *)slot;
        case 2: return *(const uint16_t *)slot;
        case 4: return *(const uint32_t *)slot;
        default: return *(const int64_t *)slot;
    }
}

int64_t mire_read_key_scalar(const MireDict *dict, int64_t entry_index) {
    const void *slot = mire_key_slot((MireDict *)dict, entry_index);
    switch (dict->key_size) {
        case 1: return *(const uint8_t *)slot;
        case 2: return *(const uint16_t *)slot;
        case 4: return *(const uint32_t *)slot;
        default: return *(const int64_t *)slot;
    }
}

void *mire_read_ptr(const MireDict *dict, int64_t entry_index) {
    return *(void **)mire_value_slot((MireDict *)dict, entry_index);
}

void *mire_read_key_ptr(const MireDict *dict, int64_t entry_index) {
    return *(void **)mire_key_slot((MireDict *)dict, entry_index);
}

// ── Bucket management ────────────────────────────────────────────────

void mire_clear_buckets(MireDict *dict) {
    for (int64_t i = 0; i < dict->bucket_cap; i++) dict->buckets[i] = -1;
    for (int64_t i = 0; i < dict->cap; i++) dict->entries[i].next = -1;
}

int mire_rehash(MireDict *dict, int64_t bucket_cap) {
    int64_t *new_buckets = (int64_t *)calloc((size_t)bucket_cap, sizeof(int64_t));
    if (!new_buckets) return 0;
    int64_t *old_buckets = dict->buckets;
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

int mire_resize_storage(MireDict *dict, int64_t new_cap) {
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

int mire_grow_entries(MireDict *dict) {
    int64_t new_cap = dict->cap < 8 ? 8 : dict->cap * 2;
    if (!mire_resize_storage(dict, new_cap)) return 0;
    return 1;
}

int mire_maybe_grow_buckets(MireDict *dict) {
    if (dict->bucket_cap <= 0 || dict->len > dict->bucket_cap * 2)
        return mire_rehash(dict, dict->bucket_cap < 8 ? 8 : dict->bucket_cap * 2);
    return 1;
}

int64_t mire_dict_find(MireDict *dict, int64_t key_i64, const void *key_ptr, uint64_t hash) {
    if (!dict || dict->bucket_cap <= 0) return -1;
    int64_t bi = (int64_t)(hash % (uint64_t)dict->bucket_cap);
    if (bi < 0) bi = -bi;
    int64_t idx = dict->buckets[bi];
    while (idx >= 0) {
        if (dict->entries[idx].hash == (int64_t)hash && mire_key_equals(dict, idx, key_i64, key_ptr))
            return idx;
        idx = dict->entries[idx].next;
    }
    return -1;
}

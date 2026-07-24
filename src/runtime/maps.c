#include "runtime.h"
#include "maps_internal.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// ═══════════════════════════════════════════════════════════════════════
//  Hash map — public API
//
//  Internal hashing/slot/bucket helpers are in maps_internal.c.
// ═══════════════════════════════════════════════════════════════════════

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
        dict->key_size = mire_kind_size(key_kind);
        dict->value_size = mire_kind_size(value_kind);
        if (!mire_resize_storage(dict, 8)) return dict;
    }
    return dict;
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
    uint64_t h = mire_hash_key(key_kind, key_i64, key_ptr);
    int64_t idx = mire_dict_find(dict, key_i64, key_ptr, h);
    if (idx < 0) return default_value;
    return mire_read_scalar(dict, idx);
}

void *rt_dict_set_i64(void *dict_ptr, int64_t key_kind, int64_t value_kind,
                       int64_t key_i64, void *key_ptr, int64_t value)
{
    MireDict *dict = (MireDict *)rt_dict_ensure_kind(dict_ptr, key_kind, value_kind);
    if (!dict) return dict_ptr;
    uint64_t h = mire_hash_key(key_kind, key_i64, key_ptr);
    int64_t idx = mire_dict_find(dict, key_i64, key_ptr, h);
    if (idx >= 0) {
        mire_store_value(dict, idx, value, NULL, 1);
        return dict;
    }
    if (dict->len >= dict->cap) {
        if (!mire_grow_entries(dict)) return dict;
    }
    idx = dict->len;
    dict->entries[idx].hash = (int64_t)h;
    dict->entries[idx].next = -1;
    mire_store_key(dict, idx, key_i64, key_ptr, 0);
    mire_store_value(dict, idx, value, NULL, 0);
    dict->len++;
    int64_t bi = (int64_t)(h % (uint64_t)dict->bucket_cap);
    if (bi < 0) bi = -bi;
    dict->entries[idx].next = dict->buckets[bi];
    dict->buckets[bi] = idx;
    mire_maybe_grow_buckets(dict);
    return dict;
}

void *rt_dict_get_ptr(void *dict_ptr, int64_t key_kind, int64_t key_i64,
                       void *key_ptr, void *default_value)
{
    if (!dict_ptr) return default_value;
    MireDict *dict = (MireDict *)dict_ptr;
    uint64_t h = mire_hash_key(key_kind, key_i64, key_ptr);
    int64_t idx = mire_dict_find(dict, key_i64, key_ptr, h);
    if (idx < 0) return default_value;
    return mire_read_ptr(dict, idx);
}

void *rt_dict_set_ptr(void *dict_ptr, int64_t key_kind, int64_t value_kind,
                       int64_t key_i64, void *key_ptr, void *value)
{
    MireDict *dict = (MireDict *)rt_dict_ensure_kind(dict_ptr, key_kind, value_kind);
    if (!dict) return dict_ptr;
    uint64_t h = mire_hash_key(key_kind, key_i64, key_ptr);
    int64_t idx = mire_dict_find(dict, key_i64, key_ptr, h);
    if (idx >= 0) {
        mire_store_value(dict, idx, 0, value, 1);
        return dict;
    }
    if (dict->len >= dict->cap) {
        if (!mire_grow_entries(dict)) return dict;
    }
    idx = dict->len;
    dict->entries[idx].hash = (int64_t)h;
    dict->entries[idx].next = -1;
    mire_store_key(dict, idx, key_i64, key_ptr, 0);
    mire_store_value(dict, idx, 0, value, 0);
    dict->len++;
    int64_t bi = (int64_t)(h % (uint64_t)dict->bucket_cap);
    if (bi < 0) bi = -bi;
    dict->entries[idx].next = dict->buckets[bi];
    dict->buckets[bi] = idx;
    mire_maybe_grow_buckets(dict);
    return dict;
}

int64_t rt_dict_has(void *dict_ptr, int64_t key_kind, int64_t key_i64, void *key_ptr) {
    if (!dict_ptr) return 0;
    MireDict *dict = (MireDict *)dict_ptr;
    uint64_t h = mire_hash_key(key_kind, key_i64, key_ptr);
    return mire_dict_find(dict, key_i64, key_ptr, h) >= 0 ? 1 : 0;
}

void *rt_dict_remove(void *dict_ptr, int64_t key_kind, int64_t key_i64, void *key_ptr) {
    if (!dict_ptr) return dict_ptr;
    MireDict *dict = (MireDict *)dict_ptr;
    uint64_t h = mire_hash_key(key_kind, key_i64, key_ptr);
    int64_t bi = (int64_t)(h % (uint64_t)dict->bucket_cap);
    if (bi < 0) bi = -bi;
    int64_t prev = -1;
    int64_t idx = dict->buckets[bi];
    while (idx >= 0) {
        if (dict->entries[idx].hash == (int64_t)h && mire_key_equals(dict, idx, key_i64, key_ptr)) {
            if (prev < 0) dict->buckets[bi] = dict->entries[idx].next;
            else dict->entries[prev].next = dict->entries[idx].next;
            dict->len--;
            if (idx != dict->len) {
                if (dict->key_kind == MIRE_KIND_STR) {
                    char *old_key = *(char **)mire_key_slot(dict, idx);
                    if (old_key) {
                        if (rt_managed_is_managed(old_key)) rt_managed_free(old_key);
                        else free(old_key);
                    }
                }
                if (dict->value_kind == MIRE_KIND_STR) {
                    char *old_val = *(char **)mire_value_slot(dict, idx);
                    if (old_val) {
                        if (rt_managed_is_managed(old_val)) rt_managed_free(old_val);
                        else free(old_val);
                    }
                }
                dict->entries[idx] = dict->entries[dict->len];
                memcpy(mire_key_slot(dict, idx), mire_key_slot(dict, dict->len), (size_t)dict->key_size);
                memcpy(mire_value_slot(dict, idx), mire_value_slot(dict, dict->len), (size_t)dict->value_size);
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
        const char *s = *(const char **)mire_key_slot((MireDict *)dict, entry_index);
        size_t len = strlen(s);
        char *out = (char *)malloc(len + 3);
        if (!out) return rt_strdup_raw("");
        out[0] = '"';
        memcpy(out + 1, s, len);
        out[len + 1] = '"';
        out[len + 2] = '\0';
        return out;
    }
    return format_scalar(mire_read_key_scalar(dict, entry_index), dict->key_kind);
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
            const char *s = *(const char **)mire_value_slot(dict, i);
            size_t slen = strlen(s);
            v = (char *)malloc(slen + 3);
            if (v) { v[0] = '"'; memcpy(v + 1, s, slen); v[slen + 1] = '"'; v[slen + 2] = '\0'; }
        } else if (dict->value_kind == MIRE_KIND_MAP) {
            void *sub = mire_read_ptr(dict, i);
            v = rt_dict_to_string(sub);
        } else {
            v = format_scalar(mire_read_scalar(dict, i), dict->value_kind);
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
            const char *s = *(const char **)mire_value_slot(dict, i);
            size_t slen = strlen(s);
            v = (char *)malloc(slen + 3);
            if (v) { v[0] = '"'; memcpy(v + 1, s, slen); v[slen + 1] = '"'; v[slen + 2] = '\0'; }
        } else if (dict->value_kind == MIRE_KIND_MAP) {
            v = rt_dict_to_string(mire_read_ptr(dict, i));
        } else {
            v = format_scalar(mire_read_scalar(dict, i), dict->value_kind);
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
        void *k = mire_read_key_ptr(dict, i);
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
            void *v = mire_read_ptr(dict, i);
            list = rt_list_push_ptr(list, v);
        } else {
            int64_t v = mire_read_scalar(dict, i);
            list = rt_list_push_i64(list, v);
        }
    }
    return list;
}

void rt_dict_free(void *dict_ptr) {
    if (!dict_ptr) return;
    MireDict *dict = (MireDict *)dict_ptr;
    if (dict->key_kind == MIRE_KIND_STR) {
        for (int64_t i = 0; i < dict->len; i++) {
            char *key = *(char **)(dict->key_storage + i * dict->key_size);
            if (key) {
                if (rt_managed_is_managed(key)) rt_managed_free(key);
                else free(key);
            }
        }
    }
    if (dict->value_kind == MIRE_KIND_STR) {
        for (int64_t i = 0; i < dict->len; i++) {
            char *val = *(char **)(dict->value_storage + i * dict->value_size);
            if (val) {
                if (rt_managed_is_managed(val)) rt_managed_free(val);
                else free(val);
            }
        }
    }
    free(dict->buckets);
    free(dict->entries);
    free(dict->key_storage);
    free(dict->value_storage);
    free(dict);
}

// ── Dicts module aliases (rt_dicts_*) ────────────────────────────────
int64_t rt_dicts_len(void *dict) { return rt_dict_len(dict); }
void *rt_dicts_get(void *dict, const char *key) {
    return rt_dict_get_ptr(dict, 3, 0, (void *)key, NULL);
}
int64_t rt_dicts_get_i64(void *dict, const char *key) {
    return rt_dict_get_i64(dict, 3, 0, (void *)key, 0);
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

// ── Maps module aliases (rt_maps_*) ──────────────────────────────────
int64_t rt_maps_len(void *map) { return rt_dict_len(map); }
void   *rt_maps_get(void *map, const char *key) {
    return rt_dict_get_ptr(map, 3, 0, (void *)key, NULL);
}
int64_t rt_maps_get_i64(void *map, const char *key) {
    return rt_dict_get_i64(map, 3, 0, (void *)key, 0);
}
void   *rt_maps_set(void *map, const char *key, void *value) {
    return rt_dict_set_ptr(map, 3, map ? ((MireDict *)map)->value_kind : 3, 0, (void *)key, value);
}
void   *rt_maps_set_with_kind(void *map, const char *key, void *value, int64_t value_kind) {
    return rt_dict_set_ptr(map, 3, value_kind, 0, (void *)key, value);
}
void   *rt_maps_set_i64(void *map, const char *key, int64_t value) {
    return rt_dict_set_i64(map, 3, 1, 0, (void *)key, value);
}
int64_t rt_maps_has(void *map, const char *key) {
    return rt_dict_has(map, 3, 0, (void *)key);
}
void   *rt_maps_remove(void *map, const char *key) {
    return rt_dict_remove(map, 3, 0, (void *)key);
}
void   *rt_maps_keys(void *map) { return rt_dict_keys(map); }
void   *rt_maps_values(void *map) { return rt_dict_values(map); }
int64_t rt_maps_entries(void *map) { return rt_dict_len(map); }
void   *rt_maps_merge(void *a, void *b) { return rt_dicts_merge(a, b); }
int64_t rt_maps_is_empty(void *map) { return rt_dict_len(map) <= 0 ? 1 : 0; }

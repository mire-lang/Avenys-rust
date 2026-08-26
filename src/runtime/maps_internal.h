#ifndef MIRE_MAPS_INTERNAL_H
#define MIRE_MAPS_INTERNAL_H

#include "runtime.h"
#include <stdint.h>

// ── Internal hash map helpers ──────────────────────────────────────
// Used by maps.c (public API). Not part of the external ABI.

uint64_t mire_hash_string(const char *src);
uint64_t mire_hash_u64(uint64_t value);
uint64_t mire_hash_key(int64_t key_kind, int64_t key_i64, const void *key_ptr);

int64_t  mire_kind_size(int64_t kind);
void    *mire_key_slot(MireDict *dict, int64_t index);
void    *mire_value_slot(MireDict *dict, int64_t index);
void     mire_write_scalar(void *slot, int64_t size, int64_t value);
int      mire_key_equals(const MireDict *dict, int64_t entry_index,
                          int64_t key_i64, const void *key_ptr);
void     mire_store_key(MireDict *dict, int64_t entry_index,
                         int64_t key_i64, const void *key_ptr, int replacing);
void     mire_store_value(MireDict *dict, int64_t entry_index,
                           int64_t value_i64, const void *value_ptr, int replacing);
int64_t  mire_read_scalar(const MireDict *dict, int64_t entry_index);
int64_t  mire_read_key_scalar(const MireDict *dict, int64_t entry_index);
void    *mire_read_ptr(const MireDict *dict, int64_t entry_index);
void    *mire_read_key_ptr(const MireDict *dict, int64_t entry_index);
void     mire_clear_buckets(MireDict *dict);
int      mire_rehash(MireDict *dict, int64_t bucket_cap);
int      mire_resize_storage(MireDict *dict, int64_t new_cap);
int      mire_grow_entries(MireDict *dict);
int      mire_maybe_grow_buckets(MireDict *dict);
int64_t  mire_dict_find(MireDict *dict, int64_t key_i64, const void *key_ptr, uint64_t hash);

#endif // MIRE_MAPS_INTERNAL_H

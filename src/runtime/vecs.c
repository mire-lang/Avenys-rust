#include "runtime.h"
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

// Fast list implementation - inline storage
// Format: [capacity, length, data...]

static int list_allocation_size(int64_t capacity, int64_t elem_size, size_t *out) {
    if (capacity < 0 || elem_size <= 0) return 0;
    size_t cap = (size_t)capacity;
    size_t size = (size_t)elem_size;
    if (cap > (SIZE_MAX - 16) / size) return 0;
    *out = 16 + cap * size;
    return 1;
}

void *rt_list_create(int64_t initial_cap, int64_t elem_size) {
    if (elem_size <= 0) return NULL;
    if (initial_cap < 4) initial_cap = 4;
    size_t allocation_size;
    if (!list_allocation_size(initial_cap, elem_size, &allocation_size)) return NULL;
    int64_t *ptr = (int64_t *)malloc(allocation_size);
    if (!ptr) return NULL;
    ptr[0] = initial_cap;
    ptr[1] = 0;
    return ptr + 1;
}

int64_t rt_list_len(void *list_ptr) {
    if (!list_ptr) return 0;
    return ((int64_t *)list_ptr)[0];
}

static int64_t list_cap(void *list_ptr) {
    if (!list_ptr) return 0;
    return ((int64_t *)list_ptr)[-1];
}

static void *list_grow(void *list_ptr, int64_t elem_size) {
    int64_t old_cap = list_cap(list_ptr);
    int64_t old_len = rt_list_len(list_ptr);
    if (old_cap > INT64_MAX - (old_cap >> 1)) return NULL;
    int64_t new_cap = old_cap < 4 ? 4 : old_cap + (old_cap >> 1);
    size_t allocation_size;
    if (!list_allocation_size(new_cap, elem_size, &allocation_size)) return NULL;
    int64_t *old_ptr = ((int64_t *)list_ptr) - 1;
    int64_t *new_ptr = (int64_t *)realloc(old_ptr, allocation_size);
    if (!new_ptr) return NULL;
    new_ptr[0] = new_cap;
    new_ptr[1] = old_len;
    return new_ptr + 1;
}

void *rt_list_push_i64(void *list_ptr, int64_t value) {
    if (!list_ptr) {
        list_ptr = rt_list_create(4, 8);
        if (!list_ptr) return NULL;
    }
    int64_t len = rt_list_len(list_ptr);
    int64_t cap = list_cap(list_ptr);
    if (len >= cap) {
        list_ptr = list_grow(list_ptr, 8);
        if (!list_ptr) return NULL;
    }
    ((int64_t *)list_ptr)[len + 1] = value;
    ((int64_t *)list_ptr)[0] = len + 1;
    return list_ptr;
}

void *rt_list_push_ptr(void *list_ptr, void *value) {
    if (!list_ptr) {
        list_ptr = rt_list_create(4, 8);
        if (!list_ptr) return NULL;
    }
    int64_t len = rt_list_len(list_ptr);
    int64_t cap = list_cap(list_ptr);
    if (len >= cap) {
        list_ptr = list_grow(list_ptr, 8);
        if (!list_ptr) return NULL;
    }
    ((void **)list_ptr)[len + 1] = value;
    ((int64_t *)list_ptr)[0] = len + 1;
    return list_ptr;
}

void *rt_list_push_scalar(void *list_ptr, int64_t value, int64_t elem_size) {
    if (elem_size <= 0) return list_ptr;
    if (!list_ptr) {
        list_ptr = rt_list_create(4, elem_size);
        if (!list_ptr) return NULL;
    }
    int64_t len = rt_list_len(list_ptr);
    int64_t cap = list_cap(list_ptr);
    if (len >= cap) {
        list_ptr = list_grow(list_ptr, elem_size);
        if (!list_ptr) return NULL;
    }
    if (elem_size == 8) {
        ((int64_t *)list_ptr)[len + 1] = value;
    } else if (elem_size == 4) {
        *(int32_t *)((char *)list_ptr + 8 + len * 4) = (int32_t)value;
    } else if (elem_size == 2) {
        *(int16_t *)((char *)list_ptr + 8 + len * 2) = (int16_t)value;
    } else if (elem_size == 1) {
        *((int8_t *)list_ptr + 8 + len) = (int8_t)value;
    } else {
        memcpy((char *)list_ptr + 8 + len * elem_size, &value, elem_size);
    }
    ((int64_t *)list_ptr)[0] = len + 1;
    return list_ptr;
}

int64_t rt_list_pop_i64(void *list_ptr) {
    int64_t len = rt_list_len(list_ptr);
    if (len <= 0) return 0;
    ((int64_t *)list_ptr)[0] = len - 1;
    return ((int64_t *)list_ptr)[len];
}

void *rt_list_concat(void *left_ptr, void *right_ptr) {
    int64_t llen = rt_list_len(left_ptr);
    int64_t rlen = rt_list_len(right_ptr);
    if (rlen > INT64_MAX - llen) return left_ptr;
    int64_t total = llen + rlen;
    size_t allocation_size;
    if (!list_allocation_size(total, 8, &allocation_size)) return left_ptr;
    int64_t *result = (int64_t *)malloc(allocation_size);
    if (!result) return left_ptr;
    result[0] = total;  // capacity
    result[1] = total;  // length
    int64_t *out = result + 2;
    if (left_ptr) {
        int64_t *larr = (int64_t *)left_ptr + 1;
        for (int64_t i = 0; i < llen; i++) {
            out[i] = larr[i];
        }
    }
    if (right_ptr) {
        int64_t *rarr = (int64_t *)right_ptr + 1;
        for (int64_t i = 0; i < rlen; i++) {
            out[llen + i] = rarr[i];
        }
    }
    // Return a pointer that conforms to the list layout:
    // list_ptr[-1] = cap, list_ptr[0] = len, list_ptr[1..] = data
    // result[0]=cap, result[1]=len → return result+1
    return result + 1;
}

void *rt_list_slice(void *list_ptr, int64_t start, int64_t end) {
    int64_t len = rt_list_len(list_ptr);
    if (start < 0) start = 0;
    if (end > len) end = len;
    if (start >= end) return rt_list_create(4, 8);
    int64_t new_len = end - start;
    size_t allocation_size;
    if (!list_allocation_size(new_len, 8, &allocation_size)) return rt_list_create(4, 8);
    int64_t *result = (int64_t *)malloc(allocation_size);
    if (!result) return rt_list_create(4, 8);
    result[0] = new_len;  // capacity
    result[1] = new_len;  // length
    int64_t *out = result + 2;
    int64_t *arr = (int64_t *)list_ptr + 1;
    for (int64_t i = 0; i < new_len; i++) {
        out[i] = arr[start + i];
    }
    return result + 1;
}

void *rt_list_remove(void *list_ptr, int64_t index) {
    int64_t len = rt_list_len(list_ptr);
    if (index < 0 || index >= len) return list_ptr;
    int64_t *data = (int64_t *)list_ptr + 1;
    for (int64_t i = index; i < len - 1; i++) data[i] = data[i + 1];
    ((int64_t *)list_ptr)[0] = len - 1;
    return list_ptr;
}

void rt_list_free(void *list_ptr) {
    if (!list_ptr) return;
    free(((int64_t *)list_ptr) - 1);
}

void *rt_list_clear(void *list_ptr) {
    if (list_ptr) {
        ((int64_t *)list_ptr)[0] = 0;
    }
    return list_ptr;
}

int64_t rt_list_get_i64(void *list_ptr, int64_t index) {
    int64_t len = rt_list_len(list_ptr);
    if (index < 0 || index >= len) return 0;
    return ((int64_t *)list_ptr)[index + 1];
}

void *rt_list_get_ptr(void *list_ptr, int64_t index) {
    int64_t len = rt_list_len(list_ptr);
    if (index < 0 || index >= len) return NULL;
    void *value = ((void **)list_ptr)[index + 1];
    return value;
}

void rt_list_set_i64(void *list_ptr, int64_t index, int64_t value) {
    int64_t len = rt_list_len(list_ptr);
    if (!list_ptr || index < 0 || index >= len) return;
    ((int64_t *)list_ptr)[index + 1] = value;
}
void rt_lists_set_i64(void *list, int64_t index, int64_t value) { rt_list_set_i64(list, index, value); }

int64_t rt_lists_len(void *list) { return rt_list_len(list); }
int64_t rt_lists_get_i64(void *list, int64_t index) { return rt_list_get_i64(list, index); }
void *rt_lists_get_ptr(void *list, int64_t index) { return rt_list_get_ptr(list, index); }
char *rt_vec_get_str(void *list, int64_t index) { char *raw = (char *)rt_list_get_ptr(list, index); return rt_managed_ensure_managed(raw); }
int64_t rt_vec_len(void *list) { return rt_list_len(list); }
void *rt_lists_push_i64(void *list, int64_t value) { return rt_list_push_i64(list, value); }
void *rt_lists_push_ptr(void *list, void *value) { return rt_list_push_ptr(list, value); }
int64_t rt_lists_pop(void *list) { return rt_list_pop_i64(list); }
void *rt_lists_slice(void *list, int64_t start, int64_t end) { return rt_list_slice(list, start, end); }
void *rt_lists_concat(void *a, void *b) { return rt_list_concat(a, b); }
void *rt_lists_remove(void *list, int64_t index) { return rt_list_remove(list, index); }
void *rt_lists_clear(void *list) { return rt_list_clear(list); }
void *rt_lists_flatten(void *list) {
    void *result = rt_list_create(8, 8);
    int64_t len = rt_list_len(list);
    for (int64_t i = 0; i < len; i++) {
        void *sublist = rt_list_get_ptr(list, i);
        int64_t sublen = rt_list_len(sublist);
        for (int64_t j = 0; j < sublen; j++)
            result = rt_list_push_i64(result, rt_list_get_i64(sublist, j));
    }
    return result;
}
void *rt_lists_sort(void *list) {
    int64_t len = rt_list_len(list);
    int64_t *arr = (int64_t *)list + 1;
    for (int64_t i = 1; i < len; i++) {
        int64_t key = arr[i];
        int64_t j = i - 1;
        while (j >= 0 && arr[j] > key) { arr[j + 1] = arr[j]; j--; }
        arr[j + 1] = key;
    }
    return list;
}
char *rt_lists_join_list(void *list, const char *sep) {
    return rt_strings_join_list(list, sep);
}
int64_t rt_lists_first(void *list, int64_t line, int64_t col, const char *file) {
    if (rt_list_len(list) <= 0)
        rt_panic_loc("called first() on empty vec", line, col, file);
    return rt_list_get_i64(list, 0);
}
int64_t rt_lists_last(void *list, int64_t line, int64_t col, const char *file) {
    int64_t len = rt_list_len(list);
    if (len <= 0)
        rt_panic_loc("called last() on empty vec", line, col, file);
    return rt_list_get_i64(list, len - 1);
}

int64_t rt_lists_contains_i64(void *list, int64_t needle) {
    int64_t len = rt_list_len(list);
    int64_t *data = (int64_t *)list + 1;
    for (int64_t i = 0; i < len; i++) {
        if (data[i] == needle) return 1;
    }
    return 0;
}

int64_t rt_lists_index_of_i64(void *list, int64_t needle) {
    int64_t len = rt_list_len(list);
    int64_t *data = (int64_t *)list + 1;
    for (int64_t i = 0; i < len; i++) {
        if (data[i] == needle) return i;
    }
    return -1;
}

void *rt_lists_reverse(void *list) {
    int64_t len = rt_list_len(list);
    void *result = rt_list_create(len > 0 ? len : 4, 8);
    if (!result) return NULL;
    int64_t *out = (int64_t *)result + 1;
    int64_t *data = (int64_t *)list + 1;
    for (int64_t i = 0; i < len; i++) {
        out[i] = data[len - 1 - i];
    }
    ((int64_t *)result)[0] = len;
    return result;
}

void *rt_lists_unique(void *list) {
    int64_t len = rt_list_len(list);
    void *result = rt_list_create(len > 0 ? len : 4, 8);
    if (!result) return NULL;
    int64_t *data = (int64_t *)list + 1;
    int64_t *out = (int64_t *)result + 1;
    int64_t out_len = 0;
    for (int64_t i = 0; i < len; i++) {
        int64_t value = data[i];
        int seen = 0;
        for (int64_t j = 0; j < out_len; j++) {
            if (out[j] == value) {
                seen = 1;
                break;
            }
        }
        if (!seen) {
            out[out_len++] = value;
        }
    }
    ((int64_t *)result)[0] = out_len;
    return result;
}

// ── Vecs module aliases (rt_vecs_*) ──────────────────────────────────
int64_t rt_vecs_len(void *vec) { return rt_list_len(vec); }
int64_t rt_vecs_get_i64(void *vec, int64_t index) { return rt_list_get_i64(vec, index); }
void   *rt_vecs_get_ptr(void *vec, int64_t index) { return rt_list_get_ptr(vec, index); }
void   *rt_vecs_push_i64(void *vec, int64_t value) { return rt_list_push_i64(vec, value); }
void   *rt_vecs_push_ptr(void *vec, void *value) { return rt_list_push_ptr(vec, value); }
void    rt_vecs_set_i64(void *vec, int64_t index, int64_t value) { rt_list_set_i64(vec, index, value); }
int64_t rt_vecs_pop(void *vec) { return rt_list_pop_i64(vec); }
void   *rt_vecs_slice(void *vec, int64_t start, int64_t end) { return rt_list_slice(vec, start, end); }
void   *rt_vecs_concat(void *a, void *b) { return rt_list_concat(a, b); }
void   *rt_vecs_remove(void *vec, int64_t index) { return rt_list_remove(vec, index); }
void   *rt_vecs_clear(void *vec) { return rt_list_clear(vec); }
void   *rt_vecs_flatten(void *vec) { return rt_lists_flatten(vec); }
void   *rt_vecs_sort(void *vec) { return rt_lists_sort(vec); }
void   *rt_vecs_reverse(void *vec) { return rt_lists_reverse(vec); }
void   *rt_vecs_unique(void *vec) { return rt_lists_unique(vec); }
int64_t rt_vecs_first(void *vec, int64_t line, int64_t col, const char *file) {
    return rt_lists_first(vec, line, col, file);
}
int64_t rt_vecs_last(void *vec, int64_t line, int64_t col, const char *file) {
    return rt_lists_last(vec, line, col, file);
}
int64_t rt_vecs_contains_i64(void *vec, int64_t needle) { return rt_lists_contains_i64(vec, needle); }
int64_t rt_vecs_index_of_i64(void *vec, int64_t needle) { return rt_lists_index_of_i64(vec, needle); }

void *rt_vecs_clone(void *vec) {
    if (!vec) return NULL;
    int64_t len = rt_list_len(vec);
    if (len == 0) return rt_list_create(0, 8);
    int64_t cap = list_cap((void *)vec);
    int64_t elem_size = 8;
    void *new_vec = rt_list_create(len, 8);
    if (!new_vec) return NULL;
    memcpy(((char *)new_vec) + 16, ((char *)vec) + 16, (size_t)len * 8);
    ((int64_t *)new_vec)[1] = len;
    return new_vec;
}

// ── filter ───────────────────────────────────────────────────────
void *rt_vecs_filter_i64(void *vec, int64_t (*pred)(int64_t)) {
    if (!vec) return rt_list_create(4, 8);
    int64_t len = rt_list_len(vec);
    void *result = rt_list_create(len, 8);
    if (!result) return rt_list_create(4, 8);
    int64_t *src = (int64_t *)vec + 1;
    int64_t *out = (int64_t *)result + 1;
    int64_t out_len = 0;
    for (int64_t i = 0; i < rt_list_len(vec); i++) {
        if (pred(((int64_t *)vec)[i + 1])) {
            ((int64_t *)result)[out_len + 1] = ((int64_t *)vec)[i + 1];
            out_len++;
        }
    }
    ((int64_t *)result)[0] = out_len;
    return result;
}

void *rt_vecs_filter_ptr(void *vec, int64_t (*pred)(void *)) {
    if (!vec) return rt_list_create(4, 8);
    int64_t len = rt_list_len(vec);
    void *result = rt_list_create(len, 8);
    if (!result) return rt_list_create(4, 8);
    void **src = (void **)vec + 1;
    void **out = (void **)result + 1;
    int64_t out_len = 0;
    for (int64_t i = 0; i < rt_list_len(vec); i++) {
        if (pred(((void **)vec)[i + 1])) {
            ((void **)result)[out_len + 1] = ((void **)vec)[i + 1];
            out_len++;
        }
    }
    ((int64_t *)result)[0] = out_len;
    return result;
}

// ── map ──────────────────────────────────────────────────────────
void *rt_vecs_map_i64_i64(void *vec, int64_t (*f)(int64_t)) {
    if (!vec) return rt_list_create(4, 8);
    int64_t len = rt_list_len(vec);
    void *result = rt_list_create(len, 8);
    if (!result) return rt_list_create(4, 8);
    int64_t *src = (int64_t *)vec + 1;
    int64_t *out = (int64_t *)result + 1;
    for (int64_t i = 0; i < len; i++) {
        out[i] = f(src[i]);
    }
    ((int64_t *)result)[0] = len;
    return result;
}

void *rt_vecs_map_i64_ptr(void *vec, void *(*f)(int64_t)) {
    if (!vec) return rt_list_create(4, 8);
    int64_t len = rt_list_len(vec);
    void *result = rt_list_create(len, 8);
    if (!result) return rt_list_create(4, 8);
    int64_t *src = (int64_t *)vec + 1;
    void **out = (void **)result + 1;
    for (int64_t i = 0; i < len; i++) {
        out[i] = f(src[i]);
    }
    ((int64_t *)result)[0] = len;
    return result;
}

void *rt_vecs_map_ptr_ptr(void *vec, void *(*f)(void *)) {
    if (!vec) return rt_list_create(4, 8);
    int64_t len = rt_list_len(vec);
    void *result = rt_list_create(len, 8);
    if (!result) return rt_list_create(4, 8);
    void **src = (void **)vec + 1;
    void **out = (void **)result + 1;
    for (int64_t i = 0; i < len; i++) {
        out[i] = f(src[i]);
    }
    ((int64_t *)result)[0] = len;
    return result;
}

// ── fold ─────────────────────────────────────────────────────────
int64_t rt_vecs_fold_i64(void *vec, int64_t init, int64_t (*f)(int64_t, int64_t)) {
    if (!vec) return 0;
    int64_t acc = 0;
    int64_t len = rt_list_len(vec);
    int64_t *data = (int64_t *)vec + 1;
    for (int64_t i = 0; i < len; i++) {
        acc = f(acc, ((int64_t *)vec)[i + 1]);
    }
    return acc;
}

void *rt_vecs_fold_ptr(void *vec, void *init, void *(*f)(void *, void *)) {
    if (!vec) return NULL;
    void *acc = init;
    int64_t len = rt_list_len(vec);
    void **data = (void **)vec + 1;
    for (int64_t i = 0; i < len; i++) {
        acc = f(acc, ((void **)vec)[i + 1]);
    }
    return acc;
}

// ── find ─────────────────────────────────────────────────────────
int64_t rt_vecs_find_i64(void *vec, int64_t (*pred)(int64_t)) {
    if (!vec) return -1;
    int64_t len = rt_list_len(vec);
    int64_t *data = (int64_t *)vec + 1;
    for (int64_t i = 0; i < len; i++) {
        if (pred(data[i])) return i;
    }
    return -1;
}

int64_t rt_vecs_find_ptr(void *vec, int64_t (*pred)(void *)) {
    if (!vec) return -1;
    int64_t len = rt_list_len(vec);
    void **data = (void **)vec + 1;
    for (int64_t i = 0; i < len; i++) {
        if (pred(((void **)vec)[i + 1])) return i;
    }
    return -1;
}

// ── partition ────────────────────────────────────────────────────
void *rt_vecs_partition_i64(void *vec, int64_t (*pred)(int64_t)) {
    if (!vec) {
        void *true_list = rt_list_create(4, 8);
        void *false_list = rt_list_create(4, 8);
        return rt_list_concat(true_list, false_list);
    }
    int64_t len = rt_list_len(vec);
    void *true_list = rt_list_create(len, 8);
    void *false_list = rt_list_create(len, 8);
    int64_t *src = (int64_t *)vec + 1;
    for (int64_t i = 0; i < rt_list_len(vec); i++) {
        if (pred(((int64_t *)vec)[i + 1])) {
            rt_list_push_i64(true_list, src[i]);
        } else {
            rt_list_push_i64(false_list, ((int64_t *)vec)[i + 1]);
        }
    }
    // Return tuple as a pair (two vectors concatenated)
    // For now, return concatenated: true elements followed by false elements
    return rt_list_concat(true_list, false_list);
}

// ── chunk ────────────────────────────────────────────────────────
void *rt_vecs_chunk(void *vec, int64_t size) {
    if (!vec || size <= 0) return rt_list_create(4, 8);
    int64_t len = rt_list_len(vec);
    int64_t num_chunks = (len + size - 1) / size;
    void *result = rt_list_create(num_chunks, 8);
    if (!result) return rt_list_create(4, 8);
    int64_t *src = (int64_t *)vec + 1;
    for (int64_t i = 0; i < num_chunks; i++) {
        int64_t start = i * 8;
        int64_t end = start + size;
        if (end > len) end = len;
        void *chunk = rt_list_create(end - start, 8);
        for (int64_t j = start; j < end; j++) {
            rt_list_push_i64(chunk, ((int64_t *)vec)[j + 1]);
        }
        rt_list_push_ptr(result, chunk);
    }
    return result;
}

// ── window ───────────────────────────────────────────────────────
void *rt_vecs_window(void *vec, int64_t size) {
    if (!vec || size <= 0) return rt_list_create(4, 8);
    int64_t len = rt_list_len(vec);
    if (len < size) return rt_list_create(4, 8);
    int64_t num_windows = len - size + 1;
    void *result = rt_list_create(num_windows, 8);
    if (!result) return rt_list_create(4, 8);
    int64_t *src = (int64_t *)vec + 1;
    for (int64_t i = 0; i < num_windows; i++) {
        void *window = rt_list_create(size, 8);
        for (int64_t j = 0; j < size; j++) {
            rt_list_push_i64(window, ((int64_t *)vec)[i + j + 1]);
        }
        rt_list_push_ptr(result, window);
    }
    return result;
}

// ── binary_search ────────────────────────────────────────────────
int64_t rt_vecs_binary_search(void *vec, int64_t value) {
    if (!vec) return -1;
    int64_t len = rt_list_len(vec);
    int64_t *data = (int64_t *)vec + 1;
    int64_t left = 0, right = rt_list_len(vec) - 1;
    while (left <= right) {
        int64_t mid = left + (right - left) / 2;
        if (data[mid] == value) return mid;
        if (data[mid] < value) left = mid + 1;
        else right = mid - 1;
    }
    return -1;
}

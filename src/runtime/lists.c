#include "runtime.h"
#include <stdlib.h>
#include <string.h>

// Fast list implementation - inline storage
// Format: [capacity, length, data...]

void *rt_list_create(int64_t initial_cap, int64_t elem_size) {
    if (initial_cap < 4) initial_cap = 4;
    int64_t *ptr = (int64_t *)malloc(16 + initial_cap * elem_size);
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
    int64_t new_cap = old_cap < 4 ? 4 : old_cap + (old_cap >> 1);
    int64_t *old_ptr = ((int64_t *)list_ptr) - 1;
    int64_t *new_ptr = (int64_t *)realloc(old_ptr, 16 + new_cap * elem_size);
    if (!new_ptr) return list_ptr;
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
    if (len >= cap) list_ptr = list_grow(list_ptr, 8);
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
    if (len >= cap) list_ptr = list_grow(list_ptr, 8);
    ((void **)list_ptr)[len + 1] = value;
    ((int64_t *)list_ptr)[0] = len + 1;
    return list_ptr;
}

void *rt_list_push_scalar(void *list_ptr, int64_t value, int64_t elem_size) {
    if (!list_ptr) {
        list_ptr = rt_list_create(4, elem_size > 0 ? elem_size : 8);
        if (!list_ptr) return NULL;
    }
    int64_t len = rt_list_len(list_ptr);
    int64_t cap = list_cap(list_ptr);
    if (len >= cap) list_ptr = list_grow(list_ptr, elem_size > 0 ? elem_size : 8);
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
    int64_t total = llen + rlen;
    void *result = malloc(16 + total * 8);
    if (!result) return left_ptr;
    ((int64_t *)result)[0] = total;
    ((int64_t *)result)[1] = total;
    int64_t *out = (int64_t *)result + 2;
    int64_t *larr = (int64_t *)left_ptr + 1;
    int64_t *rarr = (int64_t *)right_ptr + 1;
    for (int64_t i = 0; i < llen; i++) out[i] = larr[i];
    for (int64_t i = 0; i < rlen; i++) out[llen + i] = rarr[i];
    return out;
}

void *rt_list_slice(void *list_ptr, int64_t start, int64_t end) {
    int64_t len = rt_list_len(list_ptr);
    if (start < 0) start = 0;
    if (end > len) end = len;
    if (start >= end) return rt_list_create(4, 8);
    int64_t new_len = end - start;
    void *result = malloc(16 + new_len * 8);
    if (!result) return rt_list_create(4, 8);
    ((int64_t *)result)[0] = new_len;
    ((int64_t *)result)[1] = new_len;
    int64_t *out = (int64_t *)result + 2;
    int64_t *arr = (int64_t *)list_ptr + 1;
    for (int64_t i = 0; i < new_len; i++) out[i] = arr[start + i];
    return out;
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
    if (list_ptr) ((int64_t *)list_ptr)[0] = 0;
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
    return ((void **)list_ptr)[index + 1];
}

int64_t rt_lists_len(void *list) { return rt_list_len(list); }
int64_t rt_lists_get_i64(void *list, int64_t index) { return rt_list_get_i64(list, index); }
void *rt_lists_get_ptr(void *list, int64_t index) { return rt_list_get_ptr(list, index); }
char *rt_vec_get_str(void *list, int64_t index) { return (char *)rt_list_get_ptr(list, index); }
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
int64_t rt_lists_first(void *list) {
    if (rt_list_len(list) <= 0) return 0;
    return rt_list_get_i64(list, 0);
}
int64_t rt_lists_last(void *list) {
    int64_t len = rt_list_len(list);
    if (len <= 0) return 0;
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

/* V11: vec of strings.
 *
 * The C leg manages a dynamic array of string pointers.
 */
#include "_common.h"
#include <string.h>
#include <stdlib.h>

typedef struct { char **data; size_t len; size_t cap; } vec_t;

static void vec_push(vec_t *v, char *s) {
    if (v->len == v->cap) {
        v->cap = v->cap ? v->cap * 2 : 8;
        v->data = (char **)realloc(v->data, v->cap * sizeof(char *));
    }
    v->data[v->len++] = s;
}

static char *vec_pop(vec_t *v) {
    if (v->len == 0) return NULL;
    return v->data[--v->len];
}

int main(void) {
    int64_t seed = SEED;
    vec_t v = {0};
    char buf[32];
    for (int64_t i = 0; i < 20000; i++) {
        int n = snprintf(buf, sizeof(buf), "item%ld", (long)i);
        char *s = (char *)malloc(n + 1);
        memcpy(s, buf, n);
        s[n] = 0;
        vec_push(&v, s);
    }
    int64_t acc = 0;
    for (int i = 0; i < 10000; i++) {
        char *s = vec_pop(&v);
        if (s) { acc = (acc + strlen(s)) % 1000003; free(s); }
    }
    for (size_t i = 0; i < v.len; i++) {
        acc = (acc + strlen(v.data[i])) % 1000003;
    }
    for (size_t i = 0; i < v.len; i++) free(v.data[i]);
    free(v.data);
    printf("%lld\n", (long long)acc);
    return 0;
}

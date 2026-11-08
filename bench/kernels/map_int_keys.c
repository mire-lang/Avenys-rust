/* M10: map with string keys (integer-like).
 *
 * The C leg uses an open-addressed map with string keys.
 */
#include "_common.h"
#include <string.h>

typedef struct { char *key; int64_t value; int occ; } slot_t;
typedef struct { slot_t *s; size_t cap; size_t len; } map_t;

static uint64_t fnv1a(const char *s, size_t len) {
    uint64_t h = 1469598103934665603ULL;
    for (size_t i = 0; i < len; i++) { h ^= (unsigned char)s[i]; h *= 1099511628211ULL; }
    return h;
}

static void map_grow(map_t *m) {
    size_t ncap = m->cap * 2;
    slot_t *ns = (slot_t *)calloc(ncap, sizeof(slot_t));
    for (size_t i = 0; i < m->cap; i++) {
        if (!m->s[i].occ) continue;
        size_t idx = (size_t)(fnv1a(m->s[i].key, strlen(m->s[i].key)) % ncap);
        while (ns[idx].occ) idx = (idx + 1) % ncap;
        ns[idx] = m->s[i];
    }
    free(m->s);
    m->s = ns;
    m->cap = ncap;
}

static slot_t *map_find(map_t *m, const char *key, size_t len, int for_insert) {
    size_t idx = (size_t)(fnv1a(key, len) % m->cap);
    while (1) {
        if (!m->s[idx].occ) {
            if (for_insert) return &m->s[idx];
            return &m->s[idx];
        }
        size_t klen = strlen(m->s[idx].key);
        if (klen == len && memcmp(m->s[idx].key, key, len) == 0) return &m->s[idx];
        idx = (idx + 1) % m->cap;
    }
}

static void map_set(map_t *m, const char *key, size_t len, int64_t value) {
    if (m->len * 4 >= m->cap * 3) map_grow(m);
    slot_t *s = map_find(m, key, len, 1);
    if (!s->occ) {
        s->occ = 1;
        s->key = (char *)malloc(len + 1);
        memcpy(s->key, key, len);
        s->key[len] = 0;
        m->len++;
    }
    s->value = value;
}

static int64_t map_get(map_t *m, const char *key, size_t len) {
    slot_t *s = map_find(m, key, len, 0);
    return s->occ ? s->value : 0;
}

int main(void) {
    int64_t seed = SEED;
    map_t m;
    m.cap = 8;
    m.len = 0;
    m.s = (slot_t *)calloc(m.cap, sizeof(slot_t));
    char key[32];
    for (int64_t i = 0; i < 100000; i++) {
        int n = snprintf(key, sizeof(key), "%ld", (long)i);
        map_set(&m, key, (size_t)n, i * 13 + 17);
    }
    int64_t acc = 0;
    for (int64_t i = 0; i < 100000; i++) {
        int n = snprintf(key, sizeof(key), "%ld", (long)i);
        acc = (acc + map_get(&m, key, (size_t)n)) % 1000003;
    }
    printf("%lld\n", (long long)acc);
    return 0;
}

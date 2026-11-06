/* M7: hash map insert + lookup.
 *
 * The C leg is the reference for the map workload: an open-addressed string
 * keyed map with linear probing, same insert/lookup semantics as the runtime
 * map, and the same checksum over the accumulated values.
 */
#include "_common.h"
#include <string.h>

typedef struct { char *key; int64_t value; int occ; } slot_t;
typedef struct { slot_t *s; size_t cap; } map_t;

static uint64_t fnv1a(const char *s, size_t len) {
    uint64_t h = 1469598103934665603ULL;
    for (size_t i = 0; i < len; i++) { h ^= (unsigned char)s[i]; h *= 1099511628211ULL; }
    return h;
}

static slot_t *map_find(map_t *m, const char *key, size_t len) {
    size_t idx = (size_t)(fnv1a(key, len) % m->cap);
    while (m->s[idx].occ) {
        size_t klen = strlen(m->s[idx].key);
        if (klen == len && memcmp(m->s[idx].key, key, len) == 0) { return &m->s[idx]; }
        idx = (idx + 1) % m->cap;
    }
    return &m->s[idx];
}

static int64_t map_get(map_t *m, const char *key, size_t len) {
    slot_t *s = map_find(m, key, len);
    return s->occ ? s->value : 0;
}

static void map_set(map_t *m, const char *key, size_t len, int64_t value) {
    slot_t *s = map_find(m, key, len);
    if (!s->occ) { s->occ = 1; s->key = (char *)malloc(len + 1); memcpy(s->key, key, len); s->key[len] = 0; }
    s->value = value;
}

int main(void) {
    int64_t seed = SEED;
    map_t m;
    m.cap = 2048;
    m.s = (slot_t *)calloc(m.cap, sizeof(slot_t));
    for (size_t i = 0; i < m.cap; i++) { m.s[i].occ = 0; m.s[i].key = NULL; m.s[i].value = 0; }
    char key[32];
    for (int64_t i = 0; i < 1024; i++) {
        int n = snprintf(key, sizeof(key), "key%d", (int)i);
        map_set(&m, key, (size_t)n, i * 7 + 1);
    }
    int64_t acc = 0;
    for (int64_t i = 0; i < 2000000; i++) {
        int n = snprintf(key, sizeof(key), "key%d", (int)(i % 1024));
        acc = (acc + map_get(&m, key, (size_t)n)) % 1000003;
    }
    printf("%lld\n", (long long)acc);
    return 0;
}

/* C1: string concatenation benchmark.
 *
 * Two scenarios: without reserved capacity and with reserved capacity.
 * C uses malloc/realloc for dynamic strings. */
#include "_common.h"
#include <string.h>

char *str_concat_free(char *a, const char *b) {
    size_t alen = a ? strlen(a) : 0;
    size_t blen = strlen(b);
    char *r = (char *)realloc(a, alen + blen + 1);
    if (!r) return NULL;
    memcpy(r + alen, b, blen + 1);
    return r;
}

int main(void) {
    int64_t seed = SEED;
    // Scenario 1: without reserved capacity
    for (int64_t i = 0; i < 50000; i++) {
        char *s = (char *)malloc(1);
        s[0] = '\0';
        for (int64_t j = 0; j < 10; j++) {
            char buf[32];
            int n = snprintf(buf, sizeof(buf), "chunk%lld", (long long)j);
            s = str_concat_free(s, buf);
        }
        seed = (seed + (int64_t)strlen(s)) % 1000003;
        free(s);
    }
    // Scenario 2: with reserved capacity
    char base[2048];
    char *p = base;
    for (int i = 0; i < 100; i++) {
        p += sprintf(p, "preallocated_base_string_");
    }
    size_t baselen = p - base;
    for (int64_t i = 0; i < 50000; i++) {
        char buf[32];
        int n = snprintf(buf, sizeof(buf), "suffix%lld", (long long)(i % 10));
        char *s = (char *)malloc(baselen + n + 1);
        memcpy(s, base, baselen);
        memcpy(s + baselen, buf, n + 1);
        seed = (seed + (int64_t)(baselen + n)) % 1000003;
        free(s);
    }
    printf("%lld\n", (long long)seed);
    return 0;
}
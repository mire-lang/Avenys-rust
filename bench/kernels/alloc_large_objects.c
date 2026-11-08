/* A2: large object allocation.
 *
 * Allocations of large blocks (4KB-1MB). C uses malloc/free directly. */
#include "_common.h"

int main(void) {
    int64_t seed = SEED;
    for (int64_t i = 0; i < 50000; i++) {
        int sz = 4096 + (i % 252) * 4096;
        int64_t *v = (int64_t *)malloc(sz);
        for (int64_t j = 0; j < 8; j++) {
            v[j] = i + j;
        }
        seed = (seed + 8 + v[0]) % 1000003;
        free(v);
    }
    printf("%lld\n", (long long)seed);
    return 0;
}
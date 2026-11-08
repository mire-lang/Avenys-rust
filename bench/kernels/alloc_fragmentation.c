/* A3: allocation fragmentation.
 *
 * Alternating allocations and frees of different sizes.
 * C uses a simple pool array with malloc/free. */
#include "_common.h"

#define POOL_SIZE 1000

int main(void) {
    int64_t seed = SEED;
    void *pool[POOL_SIZE] = {0};
    int pool_len = 0;
    for (int64_t i = 0; i < 200000; i++) {
        if (i % 7 == 0 && pool_len > 0) {
            int idx = (i * 13) % pool_len;
            free(pool[idx]);
            pool[idx] = pool[pool_len - 1];
            pool_len--;
        }
        int sz = 32 + (i % 15) * 16;
        int64_t *v = (int64_t *)malloc(sz);
        for (int64_t j = 0; j < sz / 8; j++) {
            v[j] = i + j;
        }
        pool[pool_len++] = v;
        seed = (seed + (sz / 8) + v[0]) % 1000003;
    }
    printf("%lld\n", (long long)seed);
    return 0;
}
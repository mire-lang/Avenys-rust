/* D3: cache locality benchmark.
 *
 * Sequential vs random array access patterns.
 * C uses simple arrays and loops. */
#include "_common.h"

int main(void) {
    int64_t seed = SEED;
    int size = 1000000;
    int64_t *data = (int64_t *)malloc(size * sizeof(int64_t));
    for (int i = 0; i < size; i++) {
        data[i] = i * 3 + 7;
    }
    // Phase 1: sequential access
    for (int i = 0; i < size; i++) {
        seed = (seed + data[i]) % 1000003;
    }
    // Phase 2: strided access
    for (int i = 0; i < size; i += 256) {
        seed = (seed + data[i]) % 1000003;
    }
    // Phase 3: random access (LCG)
    int64_t idx = 12345;
    for (int i = 0; i < size; i++) {
        idx = (idx * 1103515245 + 12345) % 1000000;
        seed = (seed + data[idx]) % 1000003;
    }
    free(data);
    printf("%lld\n", (long long)seed);
    return 0;
}
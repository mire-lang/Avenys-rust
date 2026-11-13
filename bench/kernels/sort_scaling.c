// Sorting scale kernel. Large enough to measure the runtime sort beyond startup.
#include "_common.h"

int main(void) {
    int64_t seed = SEED;
    for (int64_t round = 0; round < 100; round++) {
        int64_t values[10000];
        for (int64_t index = 0; index < 10000; index++) {
            values[index] = (index * 2654435761LL + round) % 1000003;
        }
        for (int64_t i = 1; i < 10000; i++) {
            int64_t key = values[i];
            int64_t j = i - 1;
            while (j >= 0 && values[j] > key) {
                values[j + 1] = values[j];
                j--;
            }
            values[j + 1] = key;
        }
        seed = (seed + values[0] + values[9999]) % 1000003;
    }
    printf("%lld\n", (long long)seed);
    return 0;
}

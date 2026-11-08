/* C2: string search benchmark.
 *
 * Pattern search in long strings. C uses strstr. */
#include "_common.h"
#include <string.h>

int main(void) {
    int64_t seed = SEED;
    const char *unit = "abcdefghijklmnopqrstuvwxyz0123456789 ";
    size_t unit_len = strlen(unit);
    size_t n = 2000;
    char *haystack = (char *)malloc(n * unit_len + 1);
    char *p = haystack;
    for (size_t i = 0; i < n; i++) {
        memcpy(p, unit, unit_len);
        p += unit_len;
    }
    *p = '\0';
    const char *patterns[] = {
        "pattern0", "pattern1", "pattern2", "pattern3",
        "pattern4", "pattern5", "pattern6", "pattern7",
        "xyz01", "mnopq", "notfound", "0123"
    };
    int n_patterns = 12;
    for (int64_t round = 0; round < 100; round++) {
        int64_t hits = 0;
        for (int j = 0; j < n_patterns; j++) {
            const char *found = strstr(haystack, patterns[j]);
            while (found) {
                hits++;
                found = strstr(found + 1, patterns[j]);
            }
        }
        seed = (seed + hits) % 1000003;
    }
    free(haystack);
    printf("%lld\n", (long long)seed);
    return 0;
}
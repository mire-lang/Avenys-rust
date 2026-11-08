/* S6: word count.
 *
 * The C leg splits a long static string by spaces.
 */
#include "_common.h"
#include <string.h>
#include <stdlib.h>

int main(void) {
    int64_t seed = SEED;
    char *text = "the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog";
    int count = 0;
    char *tok = strtok(text, " ");
    while (tok) {
        count++;
        tok = strtok(NULL, " ");
    }
    printf("%d\n", count);
    return 0;
}

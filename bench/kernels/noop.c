/* Noop kernel: trivial program for measuring startup overhead.
 * All three legs do the same minimal work. */
#include "_common.h"

int main(void) {
    printf("x\n");
    return 0;
}
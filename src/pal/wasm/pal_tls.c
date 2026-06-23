// WASM PAL — TLS stubs
// WASI has no native TLS. In browser, TLS is handled by the JS fetch API.
// These stubs are used when compiling for WASI.

#include "../pal.h"
#include <stdlib.h>

int64_t pal_tls_connect(const char *host, int64_t port) {
    (void)host; (void)port;
    return -1;
}

int pal_tls_send(int64_t fd, const char *data) {
    (void)fd; (void)data;
    return 0;
}

char *pal_tls_recv(int64_t fd, int64_t max_bytes) {
    (void)fd; (void)max_bytes;
    return NULL;
}

int pal_tls_close(int64_t fd) {
    (void)fd;
    return 0;
}

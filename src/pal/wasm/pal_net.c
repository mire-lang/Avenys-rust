// WASM PAL — Network stubs
// WASI preview 1 has no socket support.
// WASI preview 2 (wasi-sockets) is not yet standardized.
// Will be bridged via JS Emscripten or WASI sockets when available.

#include "../pal.h"
#include <stdlib.h>

int64_t pal_net_connect(const char *host, int64_t port) {
    (void)host; (void)port;
    return -1;
}

int64_t pal_net_connect_timeout(const char *host, int64_t port, int64_t timeout_ms) {
    (void)host; (void)port; (void)timeout_ms;
    return -1;
}

char *pal_net_recv(int64_t fd, int64_t max_bytes) {
    (void)fd; (void)max_bytes;
    return NULL;
}

int pal_net_send(int64_t fd, const char *data) {
    (void)fd; (void)data;
    return 0;
}

int pal_net_close(int64_t fd) {
    (void)fd;
    return 0;
}

int64_t pal_net_poll(int64_t fd, int64_t timeout_ms) {
    (void)fd; (void)timeout_ms;
    return -1;
}

int pal_net_set_nonblock(int64_t fd, int nonblock) {
    (void)fd; (void)nonblock;
    return 0;
}

char *pal_net_resolve(const char *host) {
    (void)host;
    return NULL;
}

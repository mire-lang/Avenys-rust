// WASM PAL — WebSocket stubs
// WASI has no WebSocket. Browser uses JS WebSocket API via pal_wasm bridge.

#include "../pal.h"
#include <stdlib.h>

int64_t pal_ws_connect(const char *host, int64_t port, const char *path) {
    (void)host; (void)port; (void)path;
    return -1;
}

int pal_ws_send_text(int64_t fd, const char *data) {
    (void)fd; (void)data;
    return 0;
}

char *pal_ws_recv(int64_t fd, int64_t max_bytes) {
    (void)fd; (void)max_bytes;
    return NULL;
}

int pal_ws_close(int64_t fd) {
    (void)fd;
    return 0;
}

int64_t pal_wss_connect(const char *host, int64_t port, const char *path) {
    (void)host; (void)port; (void)path;
    return -1;
}

int pal_wss_send_text(int64_t fd, const char *data) {
    (void)fd; (void)data;
    return 0;
}

char *pal_wss_recv(int64_t fd, int64_t max_bytes) {
    (void)fd; (void)max_bytes;
    return NULL;
}

int pal_wss_close(int64_t fd) {
    (void)fd;
    return 0;
}

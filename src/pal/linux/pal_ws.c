// Linux PAL — WebSocket client (WS + WSS)
// Handshake + frame encoding/decoding.
// Plain WS uses raw sockets; WSS uses OpenSSL.

#include "../pal.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <netdb.h>
#include <time.h>
#include <openssl/ssl.h>
#include <openssl/err.h>
#include <arpa/inet.h>

// ── Helpers ──────────────────────────────────────────────────────────

static void ws_random_bytes(unsigned char *buf, size_t len) {
    FILE *fh = fopen("/dev/urandom", "rb");
    if (fh) {
        fread(buf, 1, len, fh);
        fclose(fh);
        return;
    }
    srand((unsigned)time(NULL));
    for (size_t i = 0; i < len; i++)
        buf[i] = (unsigned char)(rand() & 0xFF);
}

static char *ws_base64_encode(const unsigned char *data, size_t len) {
    static const char table[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    size_t out_len = ((len + 2) / 3) * 4;
    char *out = (char *)malloc(out_len + 1);
    if (!out) return NULL;
    for (size_t i = 0, j = 0; i < len; ) {
        uint32_t t = (i < len ? data[i++] : 0) << 16;
        t |= (i < len ? data[i++] : 0) << 8;
        t |= (i < len ? data[i++] : 0);
        out[j++] = table[(t >> 18) & 0x3F];
        out[j++] = table[(t >> 12) & 0x3F];
        out[j++] = (i > len + 1 && i - 1 >= len) ? '=' : table[(t >> 6) & 0x3F];
        out[j++] = (i > len + 0 && i - 2 >= len) ? '=' : table[t & 0x3F];
    }
    out[out_len] = '\0';
    return out;
}

static int ws_send_raw(int fd, const unsigned char *data, size_t len) {
    ssize_t written = send(fd, data, len, 0);
    return (size_t)written == len ? 1 : 0;
}

static char *ws_recv_line(int fd) {
    size_t cap = 256, pos = 0;
    char *buf = (char *)malloc(cap);
    if (!buf) return NULL;
    char prev = 0;
    while (1) {
        char c;
        ssize_t n = recv(fd, &c, 1, 0);
        if (n <= 0) { free(buf); return NULL; }
        if (pos + 1 >= cap) {
            cap *= 2;
            char *nb = (char *)realloc(buf, cap);
            if (!nb) { free(buf); return NULL; }
            buf = nb;
        }
        buf[pos++] = c;
        if (prev == '\r' && c == '\n') {
            buf[pos - 2] = '\0';
            return buf;
        }
        prev = c;
    }
}

static char *ws_read_response(int fd) {
    char *line = ws_recv_line(fd);
    if (!line) return NULL;
    size_t cap = strlen(line) + 4;
    char *resp = (char *)malloc(cap);
    if (!resp) { free(line); return NULL; }
    snprintf(resp, cap, "%s\r\n", line);
    free(line);
    while (1) {
        line = ws_recv_line(fd);
        if (!line) { free(resp); return NULL; }
        if (strlen(line) == 0) { free(line); break; }
        size_t new_len = strlen(resp) + strlen(line) + 4;
        char *nb = (char *)realloc(resp, new_len);
        if (!nb) { free(line); free(resp); return NULL; }
        resp = nb;
        strcat(resp, line);
        strcat(resp, "\r\n");
        free(line);
    }
    return resp;
}

static int ws_do_handshake(int fd, const char *host, const char *path) {
    unsigned char key_bytes[16];
    ws_random_bytes(key_bytes, 16);
    char *ws_key = ws_base64_encode(key_bytes, 16);
    if (!ws_key) return 0;

    size_t req_len = strlen(path) + strlen(host) + strlen(ws_key) + 256;
    char *req = (char *)malloc(req_len);
    if (!req) { free(ws_key); return 0; }
    snprintf(req, req_len,
        "GET %s HTTP/1.1\r\n"
        "Host: %s\r\n"
        "Upgrade: websocket\r\n"
        "Connection: Upgrade\r\n"
        "Sec-WebSocket-Key: %s\r\n"
        "Sec-WebSocket-Version: 13\r\n"
        "\r\n",
        path, host, ws_key);
    free(ws_key);

    if (!ws_send_raw(fd, (unsigned char *)req, strlen(req))) {
        free(req);
        return 0;
    }
    free(req);

    char *resp = ws_read_response(fd);
    if (!resp) return 0;
    int ok = (strstr(resp, "101") != NULL || strstr(resp, "101 Switching Protocols") != NULL);
    free(resp);
    return ok;
}

// ── Frame helpers ────────────────────────────────────────────────────

static int ws_send_frame(int fd, int opcode, const unsigned char *payload, size_t len) {
    unsigned char frame[14];
    size_t hdr_len = 2;
    frame[0] = (unsigned char)(0x80 | opcode);

    if (len < 126) {
        frame[1] = (unsigned char)(0x80 | len);
    } else if (len < 65536) {
        frame[1] = (unsigned char)(0x80 | 126);
        frame[2] = (unsigned char)((len >> 8) & 0xFF);
        frame[3] = (unsigned char)(len & 0xFF);
        hdr_len = 4;
    } else {
        frame[1] = (unsigned char)(0x80 | 127);
        for (int i = 0; i < 8; i++)
            frame[2 + i] = (unsigned char)((len >> (56 - i * 8)) & 0xFF);
        hdr_len = 10;
    }

    unsigned char mask[4];
    ws_random_bytes(mask, 4);
    memcpy(frame + hdr_len, mask, 4);

    if (!ws_send_raw(fd, frame, hdr_len + 4)) return 0;
    unsigned char *masked = (unsigned char *)malloc(len);
    if (!masked) return 0;
    for (size_t i = 0; i < len; i++)
        masked[i] = payload[i] ^ mask[i % 4];
    int ok = ws_send_raw(fd, masked, len);
    free(masked);
    return ok;
}

static char *ws_read_frame(int fd, int64_t max_bytes, int *out_opcode) {
    unsigned char hdr[2];
    ssize_t n = recv(fd, hdr, 2, MSG_WAITALL);
    if (n < 2) return NULL;

    *out_opcode = hdr[0] & 0x0F;
    int masked = (hdr[1] & 0x80) != 0;
    size_t payload_len = hdr[1] & 0x7F;

    if (payload_len == 126) {
        unsigned char ext[2];
        if (recv(fd, ext, 2, MSG_WAITALL) < 2) return NULL;
        payload_len = ((size_t)ext[0] << 8) | ext[1];
    } else if (payload_len == 127) {
        unsigned char ext[8];
        if (recv(fd, ext, 8, MSG_WAITALL) < 8) return NULL;
        payload_len = 0;
        for (int i = 0; i < 8; i++)
            payload_len = (payload_len << 8) | ext[i];
    }

    if ((int64_t)payload_len > max_bytes) return NULL;

    unsigned char mask_key[4];
    if (masked) {
        if (recv(fd, mask_key, 4, MSG_WAITALL) < 4) return NULL;
    }

    char *payload = (char *)malloc(payload_len + 1);
    if (!payload) return NULL;
    size_t received = 0;
    while (received < payload_len) {
        ssize_t r = recv(fd, payload + received, payload_len - received, 0);
        if (r <= 0) { free(payload); return NULL; }
        received += (size_t)r;
    }
    payload[payload_len] = '\0';

    if (masked) {
        for (size_t i = 0; i < payload_len; i++)
            payload[i] ^= mask_key[i % 4];
    }
    return payload;
}

// ── Public API ───────────────────────────────────────────────────────

int64_t pal_ws_connect(const char *host, int64_t port, const char *path) {
    struct hostent *he = gethostbyname(host);
    if (!he) return -1;

    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) return -1;

    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons((uint16_t)port);
    memcpy(&addr.sin_addr, he->h_addr_list[0], (size_t)he->h_length);

    if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        close(fd);
        return -1;
    }

    if (!ws_do_handshake(fd, host, path)) {
        close(fd);
        return -1;
    }

    return (int64_t)fd;
}

int pal_ws_send_text(int64_t fd, const char *data) {
    if (!data) return 0;
    return ws_send_frame((int)fd, 0x1, (const unsigned char *)data, strlen(data));
}

char *pal_ws_recv(int64_t fd, int64_t max_bytes) {
    int opcode;
    char *payload = ws_read_frame((int)fd, max_bytes, &opcode);
    if (!payload) return NULL;
    if (opcode == 0x8) { free(payload); return NULL; }
    return payload;
}

int pal_ws_close(int64_t fd) {
    unsigned char close_frame[] = { 0x88, 0x80, 0x00, 0x00, 0x00, 0x00 };
    send((int)fd, close_frame, 6, 0);
    return close((int)fd) == 0 ? 1 : 0;
}

// ── WSS (TLS) variants ────────────────────────────────────────────────

static int ssl_init_done = 0;
static void wss_init_ssl(void) {
    if (!ssl_init_done) {
        SSL_load_error_strings();
        OpenSSL_add_ssl_algorithms();
        ssl_init_done = 1;
    }
}

static SSL_CTX *wss_create_ctx(void) {
    SSL_CTX *ctx = SSL_CTX_new(TLS_client_method());
    if (ctx) SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, NULL);
    return ctx;
}

static int wss_send_raw(SSL *ssl, const unsigned char *data, size_t len) {
    return SSL_write(ssl, data, (int)len) == (int)len ? 1 : 0;
}

static int wss_recv_byte(SSL *ssl, char *c) {
    return SSL_read(ssl, c, 1);
}

static char *wss_recv_line(SSL *ssl) {
    size_t cap = 256, pos = 0;
    char *buf = (char *)malloc(cap);
    if (!buf) return NULL;
    char prev = 0;
    while (1) {
        char c;
        int n = wss_recv_byte(ssl, &c);
        if (n <= 0) { free(buf); return NULL; }
        if (pos + 1 >= cap) {
            cap *= 2;
            char *nb = (char *)realloc(buf, cap);
            if (!nb) { free(buf); return NULL; }
            buf = nb;
        }
        buf[pos++] = c;
        if (prev == '\r' && c == '\n') {
            buf[pos - 2] = '\0';
            return buf;
        }
        prev = c;
    }
}

static char *wss_read_response(SSL *ssl) {
    char *line = wss_recv_line(ssl);
    if (!line) return NULL;
    size_t cap = strlen(line) + 4;
    char *resp = (char *)malloc(cap);
    if (!resp) { free(line); return NULL; }
    snprintf(resp, cap, "%s\r\n", line);
    free(line);
    while (1) {
        line = wss_recv_line(ssl);
        if (!line) { free(resp); return NULL; }
        if (strlen(line) == 0) { free(line); break; }
        size_t new_len = strlen(resp) + strlen(line) + 4;
        char *nb = (char *)realloc(resp, new_len);
        if (!nb) { free(line); free(resp); return NULL; }
        resp = nb;
        strcat(resp, line);
        strcat(resp, "\r\n");
        free(line);
    }
    return resp;
}

static int wss_do_handshake(SSL *ssl, const char *host, const char *path) {
    unsigned char key_bytes[16];
    ws_random_bytes(key_bytes, 16);
    char *ws_key = ws_base64_encode(key_bytes, 16);
    if (!ws_key) return 0;

    size_t req_len = strlen(path) + strlen(host) + strlen(ws_key) + 256;
    char *req = (char *)malloc(req_len);
    if (!req) { free(ws_key); return 0; }
    snprintf(req, req_len,
        "GET %s HTTP/1.1\r\n"
        "Host: %s\r\n"
        "Upgrade: websocket\r\n"
        "Connection: Upgrade\r\n"
        "Sec-WebSocket-Key: %s\r\n"
        "Sec-WebSocket-Version: 13\r\n"
        "\r\n",
        path, host, ws_key);
    free(ws_key);

    if (!wss_send_raw(ssl, (unsigned char *)req, strlen(req))) {
        free(req);
        return 0;
    }
    free(req);

    char *resp = wss_read_response(ssl);
    if (!resp) return 0;
    int ok = (strstr(resp, "101") != NULL);
    free(resp);
    return ok;
}

static int wss_send_frame(SSL *ssl, int opcode, const unsigned char *payload, size_t len) {
    unsigned char frame[14];
    size_t hdr_len = 2;
    frame[0] = (unsigned char)(0x80 | opcode);

    if (len < 126) {
        frame[1] = (unsigned char)(0x80 | len);
    } else if (len < 65536) {
        frame[1] = (unsigned char)(0x80 | 126);
        frame[2] = (unsigned char)((len >> 8) & 0xFF);
        frame[3] = (unsigned char)(len & 0xFF);
        hdr_len = 4;
    } else {
        frame[1] = (unsigned char)(0x80 | 127);
        for (int i = 0; i < 8; i++)
            frame[2 + i] = (unsigned char)((len >> (56 - i * 8)) & 0xFF);
        hdr_len = 10;
    }

    unsigned char mask[4];
    ws_random_bytes(mask, 4);
    memcpy(frame + hdr_len, mask, 4);

    if (!wss_send_raw(ssl, frame, hdr_len + 4)) return 0;
    unsigned char *masked = (unsigned char *)malloc(len);
    if (!masked) return 0;
    for (size_t i = 0; i < len; i++)
        masked[i] = payload[i] ^ mask[i % 4];
    int ok = wss_send_raw(ssl, masked, len);
    free(masked);
    return ok;
}

static char *wss_read_frame(SSL *ssl, int64_t max_bytes, int *out_opcode) {
    unsigned char hdr[2];
    int n = SSL_read(ssl, hdr, 2);
    if (n < 2) return NULL;

    *out_opcode = hdr[0] & 0x0F;
    int masked = (hdr[1] & 0x80) != 0;
    size_t payload_len = hdr[1] & 0x7F;

    if (payload_len == 126) {
        unsigned char ext[2];
        if (SSL_read(ssl, ext, 2) < 2) return NULL;
        payload_len = ((size_t)ext[0] << 8) | ext[1];
    } else if (payload_len == 127) {
        unsigned char ext[8];
        if (SSL_read(ssl, ext, 8) < 8) return NULL;
        payload_len = 0;
        for (int i = 0; i < 8; i++)
            payload_len = (payload_len << 8) | ext[i];
    }

    if ((int64_t)payload_len > max_bytes) return NULL;

    unsigned char mask_key[4];
    if (masked) {
        if (SSL_read(ssl, mask_key, 4) < 4) return NULL;
    }

    char *payload = (char *)malloc(payload_len + 1);
    if (!payload) return NULL;
    size_t received = 0;
    while (received < payload_len) {
        int r = SSL_read(ssl, payload + received, (int)(payload_len - received));
        if (r <= 0) { free(payload); return NULL; }
        received += (size_t)r;
    }
    payload[payload_len] = '\0';

    if (masked) {
        for (size_t i = 0; i < payload_len; i++)
            payload[i] ^= mask_key[i % 4];
    }
    return payload;
}

// ── WSS Public API ────────────────────────────────────────────────────

int64_t pal_wss_connect(const char *host, int64_t port, const char *path) {
    wss_init_ssl();

    struct hostent *he = gethostbyname(host);
    if (!he) return -1;

    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) return -1;

    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons((uint16_t)port);
    memcpy(&addr.sin_addr, he->h_addr_list[0], (size_t)he->h_length);

    if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        close(fd);
        return -1;
    }

    SSL_CTX *ctx = wss_create_ctx();
    if (!ctx) { close(fd); return -1; }

    SSL *ssl = SSL_new(ctx);
    if (!ssl) { SSL_CTX_free(ctx); close(fd); return -1; }

    SSL_set_fd(ssl, fd);
    if (SSL_connect(ssl) != 1) {
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        close(fd);
        return -1;
    }

    if (!wss_do_handshake(ssl, host, path)) {
        SSL_shutdown(ssl);
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        close(fd);
        return -1;
    }

    return (int64_t)(intptr_t)ssl;
}

int pal_wss_send_text(int64_t fd, const char *data) {
    if (!data) return 0;
    SSL *ssl = (SSL *)(intptr_t)fd;
    if (!ssl) return 0;
    return wss_send_frame(ssl, 0x1, (const unsigned char *)data, strlen(data));
}

char *pal_wss_recv(int64_t fd, int64_t max_bytes) {
    SSL *ssl = (SSL *)(intptr_t)fd;
    if (!ssl) return NULL;
    int opcode;
    char *payload = wss_read_frame(ssl, max_bytes, &opcode);
    if (!payload) return NULL;
    if (opcode == 0x8) { free(payload); return NULL; }
    return payload;
}

int pal_wss_close(int64_t fd) {
    SSL *ssl = (SSL *)(intptr_t)fd;
    if (!ssl) return 0;
    unsigned char close_frame[] = { 0x88, 0x80, 0x00, 0x00, 0x00, 0x00 };
    SSL_write(ssl, close_frame, 6);
    int sock = SSL_get_fd(ssl);
    SSL_shutdown(ssl);
    SSL_free(ssl);
    if (sock >= 0) close(sock);
    return 1;
}

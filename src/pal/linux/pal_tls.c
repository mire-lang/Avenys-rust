// Linux PAL — TLS via OpenSSL
// Links with -lssl -lcrypto

#include "../pal.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <netdb.h>
#include <openssl/ssl.h>
#include <openssl/err.h>

static int ssl_initialized = 0;

static void init_ssl(void) {
    if (!ssl_initialized) {
        SSL_load_error_strings();
        OpenSSL_add_ssl_algorithms();
        ssl_initialized = 1;
    }
}

static SSL_CTX *create_ssl_context(void) {
    const SSL_METHOD *method = TLS_client_method();
    SSL_CTX *ctx = SSL_CTX_new(method);
    if (!ctx) return NULL;
    SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, NULL);
    return ctx;
}

int64_t pal_tls_connect(const char *host, int64_t port) {
    init_ssl();

    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    char port_str[16];
    snprintf(port_str, sizeof(port_str), "%lld", (long long)port);

    struct addrinfo *res = NULL;
    if (getaddrinfo(host, port_str, &hints, &res) != 0 || !res) {
        return -1;
    }

    int fd = -1;
    for (struct addrinfo *ai = res; ai != NULL; ai = ai->ai_next) {
        fd = socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
        if (fd < 0) continue;
        if (connect(fd, ai->ai_addr, ai->ai_addrlen) == 0) break;
        close(fd);
        fd = -1;
    }
    freeaddrinfo(res);

    if (fd < 0) {
        return -1;
    }

    SSL_CTX *ctx = create_ssl_context();
    if (!ctx) {
        close(fd);
        return -1;
    }

    SSL *ssl = SSL_new(ctx);
    if (!ssl) {
        SSL_CTX_free(ctx);
        close(fd);
        return -1;
    }

    SSL_set_fd(ssl, fd);
    if (SSL_connect(ssl) != 1) {
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        close(fd);
        return -1;
    }

    return (int64_t)(intptr_t)ssl;
}

int pal_tls_send(int64_t fd, const char *data) {
    if (!data) return 0;
    SSL *ssl = (SSL *)(intptr_t)fd;
    if (!ssl) return 0;
    size_t len = strlen(data);
    int written = SSL_write(ssl, data, (int)len);
    return written == (int)len ? 1 : 0;
}

char *pal_tls_recv(int64_t fd, int64_t max_bytes) {
    if (max_bytes <= 0) max_bytes = 65536;
    SSL *ssl = (SSL *)(intptr_t)fd;
    if (!ssl) return NULL;
    char *buf = (char *)malloc((size_t)max_bytes + 1);
    if (!buf) return NULL;
    int n = SSL_read(ssl, buf, (int)max_bytes);
    if (n <= 0) {
        free(buf);
        return NULL;
    }
    buf[n] = '\0';
    return buf;
}

int pal_tls_close(int64_t fd) {
    SSL *ssl = (SSL *)(intptr_t)fd;
    if (!ssl) return 0;
    int sock_fd = SSL_get_fd(ssl);
    SSL_shutdown(ssl);
    SSL_free(ssl);
    if (sock_fd >= 0) close(sock_fd);
    return 1;
}

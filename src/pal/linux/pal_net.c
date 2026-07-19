// Linux PAL — Networking
// POSIX sockets + poll + DNS resolution.

#include "../pal.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <netdb.h>
#include <arpa/inet.h>
#include <fcntl.h>
#include <poll.h>
#include <errno.h>
#include <signal.h>

static int sigpipe_ignored = 0;

int64_t pal_net_connect(const char *host, int64_t port) {
    return pal_net_connect_timeout(host, port, 30000);
}

int64_t pal_net_connect_timeout(const char *host, int64_t port, int64_t timeout_ms) {
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

    if (timeout_ms > 0) {
        // Non-blocking connect path: try candidates until one connects.
        int fd = -1;
        for (struct addrinfo *ai = res; ai != NULL; ai = ai->ai_next) {
            fd = socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
            if (fd < 0) continue;
            int flags = fcntl(fd, F_GETFL, 0);
            if (flags >= 0) fcntl(fd, F_SETFL, flags | O_NONBLOCK);
            int ret = connect(fd, ai->ai_addr, ai->ai_addrlen);
            if (ret == 0) {
                fcntl(fd, F_SETFL, flags & ~O_NONBLOCK);
                freeaddrinfo(res);
                return (int64_t)fd;
            }
            if (ret < 0 && errno == EINPROGRESS) {
                struct pollfd pfd;
                pfd.fd = fd;
                pfd.events = POLLOUT;
                int pr = poll(&pfd, 1, (int)timeout_ms);
                if (pr > 0) {
                    int err = 0;
                    socklen_t len = sizeof(err);
                    if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &err, &len) == 0
                        && err == 0) {
                        fcntl(fd, F_SETFL, flags & ~O_NONBLOCK);
                        freeaddrinfo(res);
                        return (int64_t)fd;
                    }
                }
            }
            close(fd);
        }
        freeaddrinfo(res);
        return -1;
    }

    // Blocking connect path.
    int fd = -1;
    for (struct addrinfo *ai = res; ai != NULL; ai = ai->ai_next) {
        fd = socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
        if (fd < 0) continue;
        if (connect(fd, ai->ai_addr, ai->ai_addrlen) == 0) break;
        close(fd);
        fd = -1;
    }
    freeaddrinfo(res);
    if (fd < 0) return -1;
    return (int64_t)fd;
}

char *pal_net_recv(int64_t fd, int64_t max_bytes) {
    if (max_bytes <= 0) max_bytes = 65536;
    char *buf = (char *)malloc((size_t)max_bytes + 1);
    if (!buf) return NULL;
    ssize_t n = read((int)fd, buf, (size_t)max_bytes);
    if (n <= 0) {
        free(buf);
        return NULL;
    }
    buf[n] = '\0';
    return buf;
}

int pal_net_send(int64_t fd, const char *data) {
    if (!data) return 0;
    size_t len = strlen(data);
    ssize_t written = write((int)fd, data, len);
    return (written == (ssize_t)len) ? 1 : 0;
}

int pal_net_send_bytes(int64_t fd, const char *data, int64_t len) {
    if (!data || len <= 0) return 0;
    ssize_t written = write((int)fd, data, (size_t)len);
    return (written == (ssize_t)len) ? 1 : 0;
}

int pal_net_close(int64_t fd) {
    return close((int)fd) == 0 ? 1 : 0;
}

int64_t pal_net_poll(int64_t fd, int64_t timeout_ms) {
    struct pollfd pfd;
    pfd.fd = (int)fd;
    pfd.events = POLLIN;
    int ret = poll(&pfd, 1, (int)timeout_ms);
    return (int64_t)ret;
}

int pal_net_set_nonblock(int64_t fd, int nonblock) {
    int flags = fcntl((int)fd, F_GETFL, 0);
    if (flags < 0) return 0;
    if (nonblock)
        flags |= O_NONBLOCK;
    else
        flags &= ~O_NONBLOCK;
    return fcntl((int)fd, F_SETFL, flags) == 0 ? 1 : 0;
}

int64_t pal_net_bind(int64_t port) {
    if (!sigpipe_ignored) {
        signal(SIGPIPE, SIG_IGN);
        sigpipe_ignored = 1;
    }

    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) return -1;
    int opt = 1;
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));
    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = INADDR_ANY;
    addr.sin_port = htons((uint16_t)port);
    if (bind(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        close(fd);
        return -1;
    }
    if (listen(fd, SOMAXCONN) < 0) {
        close(fd);
        return -1;
    }
    return (int64_t)fd;
}

int64_t pal_net_accept(int64_t server_fd) {
    struct sockaddr_in client_addr;
    socklen_t len = sizeof(client_addr);
    int client_fd = accept((int)server_fd, (struct sockaddr *)&client_addr, &len);
    return (int64_t)client_fd;
}

char *pal_net_resolve(const char *host) {
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    struct addrinfo *res = NULL;
    if (getaddrinfo(host, NULL, &hints, &res) != 0 || !res) {
        return NULL;
    }

    char buf[INET6_ADDRSTRLEN];
    const char *ip = NULL;
    for (struct addrinfo *ai = res; ai != NULL; ai = ai->ai_next) {
        if (ai->ai_family == AF_INET) {
            ip = inet_ntop(AF_INET, &((struct sockaddr_in *)ai->ai_addr)->sin_addr,
                           buf, sizeof(buf));
            break;
        } else if (ai->ai_family == AF_INET6 && !ip) {
            ip = inet_ntop(AF_INET6, &((struct sockaddr_in6 *)ai->ai_addr)->sin6_addr,
                           buf, sizeof(buf));
        }
    }

    char *out = NULL;
    if (ip) {
        extern char *rt_strdup_raw(const char *src);
        out = rt_strdup_raw(ip);
    }
    freeaddrinfo(res);
    return out;
}

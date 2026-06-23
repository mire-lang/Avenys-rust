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

int64_t pal_net_connect(const char *host, int64_t port) {
    return pal_net_connect_timeout(host, port, 30000);
}

int64_t pal_net_connect_timeout(const char *host, int64_t port, int64_t timeout_ms) {
    struct hostent *he = gethostbyname(host);
    if (!he) return -1;

    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) return -1;

    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons((uint16_t)port);
    memcpy(&addr.sin_addr, he->h_addr_list[0], (size_t)he->h_length);

    if (timeout_ms > 0) {
        int flags = fcntl(fd, F_GETFL, 0);
        if (flags >= 0) fcntl(fd, F_SETFL, flags | O_NONBLOCK);
    }

    int ret = connect(fd, (struct sockaddr *)&addr, sizeof(addr));
    if (ret < 0 && errno == EINPROGRESS) {
        struct pollfd pfd;
        pfd.fd = fd;
        pfd.events = POLLOUT;
        int pr = poll(&pfd, 1, (int)timeout_ms);
        if (pr <= 0) {
            close(fd);
            return -1;
        }
        int err = 0;
        socklen_t len = sizeof(err);
        if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &err, &len) < 0 || err != 0) {
            close(fd);
            return -1;
        }
    } else if (ret < 0) {
        close(fd);
        return -1;
    }

    if (timeout_ms > 0) {
        int flags = fcntl(fd, F_GETFL, 0);
        if (flags >= 0) fcntl(fd, F_SETFL, flags & ~O_NONBLOCK);
    }
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

char *pal_net_resolve(const char *host) {
    struct hostent *he = gethostbyname(host);
    if (!he || !he->h_addr_list[0]) return NULL;
    struct in_addr addr;
    memcpy(&addr, he->h_addr_list[0], sizeof(addr));
    extern char *rt_strdup_raw(const char *src);
    return rt_strdup_raw(inet_ntoa(addr));
}

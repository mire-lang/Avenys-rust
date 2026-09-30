#include "../src/pal/pal.h"

#include <stdint.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    pal_listener_t listener = pal_listener_bind(39124, PAL_SOCKET_UDP);
    if (listener.index == 0 && listener.generation == 0) return 1;

    pal_socket_t sender = pal_socket_connect("127.0.0.1", 39124, PAL_SOCKET_UDP);
    if (sender.index == 0 && sender.generation == 0) return 2;

    const char payload[] = "pal-udp";
    char received[sizeof(payload)] = {0};
    int64_t sent = pal_socket_send(sender, payload, (int64_t)strlen(payload));
    int64_t got = pal_listener_recv(listener, received, (int64_t)sizeof(received));

    const char reply[] = "pal-reply";
    char echoed[sizeof(reply)] = {0};
    int64_t replied = pal_listener_send(listener, reply, (int64_t)strlen(reply));
    int64_t echoed_len = pal_socket_recv(sender, echoed, (int64_t)sizeof(echoed));

    pal_socket_close(sender);
    pal_listener_close(listener);

    if (sent != (int64_t)strlen(payload)) return 3;
    if (got != sent || memcmp(payload, received, (size_t)got) != 0) return 4;
    if (replied != (int64_t)strlen(reply)) return 5;
    if (echoed_len != replied || memcmp(reply, echoed, (size_t)echoed_len) != 0) return 6;
    puts("PAL UDP loopback: ok");
    return 0;
}

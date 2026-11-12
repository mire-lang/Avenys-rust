// pal_bridge.c — scalar FFI adapters for the typed PAL handle ABI.
//
// Mire's current extern ABI represents handles as i64 values. The PAL itself
// deliberately uses a typed {index, generation} structure so C callers cannot
// confuse resource families. These adapters are the narrow boundary between
// those two representations; the PAL public ABI remains typed and validated.

#include <stdint.h>
#include "runtime.h"
#include "pal.h"

static int64_t encode_handle(pal_handle_t handle) {
    return (int64_t)(((uint64_t)handle.generation << 32) | handle.index);
}

static pal_handle_t decode_handle(int64_t encoded) {
    uint64_t value = (uint64_t)encoded;
    pal_handle_t handle = {
        .index = (uint32_t)value,
        .generation = (uint32_t)(value >> 32),
    };
    return handle;
}

int64_t rt_pal_root_open(const char *path) {
    return encode_handle(pal_root_open(path));
}

void rt_pal_root_close(int64_t root) {
    pal_root_close(decode_handle(root));
}

int64_t rt_pal_file_open(int64_t root, const char *path, int32_t flags) {
    return encode_handle(pal_file_open(decode_handle(root), path, (pal_open_flags)flags));
}

int64_t rt_pal_file_read(int64_t file, char *buffer, int64_t capacity) {
    return pal_file_read(decode_handle(file), buffer, capacity);
}

int64_t rt_pal_file_write(int64_t file, const char *data, int64_t length) {
    return pal_file_write(decode_handle(file), data, length);
}

int64_t rt_pal_file_seek(int64_t file, int64_t offset, int32_t origin) {
    return pal_file_seek(decode_handle(file), offset, (pal_seek_from_t)origin);
}

int64_t rt_pal_file_size(int64_t file) {
    return pal_file_size(decode_handle(file));
}

void rt_pal_file_close(int64_t file) {
    pal_file_close(decode_handle(file));
}

int64_t rt_pal_dir_open(int64_t root, const char *path) {
    return encode_handle(pal_dir_open(decode_handle(root), path));
}

int64_t rt_pal_dir_next_name(int64_t directory, char *output, int64_t capacity) {
    return pal_dir_next_name(decode_handle(directory), output, capacity);
}

void rt_pal_dir_close(int64_t directory) {
    pal_dir_close(decode_handle(directory));
}

bool rt_pal_root_remove(int64_t root, const char *path) {
    return pal_root_remove(decode_handle(root), path);
}

int64_t rt_pal_unix_connect(const char *path, int32_t flags) {
    return encode_handle(pal_unix_connect(path, (pal_socket_flags)flags));
}

int64_t rt_pal_unix_bind(const char *path, int32_t flags) {
    return encode_handle(pal_unix_bind(path, (pal_socket_flags)flags));
}

int64_t rt_pal_listener_accept(int64_t listener) {
    return encode_handle(pal_listener_accept(decode_handle(listener)));
}

int64_t rt_pal_socket_connect(const char *host, uint16_t port, int32_t flags) {
    return encode_handle(pal_socket_connect(host, port, (pal_socket_flags)flags));
}

int64_t rt_pal_listener_bind(uint16_t port, int32_t flags) {
    return encode_handle(pal_listener_bind(port, (pal_socket_flags)flags));
}

int64_t rt_pal_socket_send(int64_t socket, const char *data, int64_t length) {
    return pal_socket_send(decode_handle(socket), data, length);
}

int64_t rt_pal_socket_recv(int64_t socket, char *buffer, int64_t capacity) {
    return pal_socket_recv(decode_handle(socket), buffer, capacity);
}

int64_t rt_pal_listener_send(int64_t listener, const char *data, int64_t length) {
    return pal_listener_send(decode_handle(listener), data, length);
}

int64_t rt_pal_listener_recv(int64_t listener, char *buffer, int64_t capacity) {
    return pal_listener_recv(decode_handle(listener), buffer, capacity);
}

void rt_pal_socket_close(int64_t socket) {
    pal_socket_close(decode_handle(socket));
}

void rt_pal_listener_close(int64_t listener) {
    pal_listener_close(decode_handle(listener));
}

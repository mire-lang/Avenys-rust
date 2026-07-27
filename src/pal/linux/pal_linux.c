#include "../pal.h"
#include "../core/pal_abi.h"
#include "../core/pal_core.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <dirent.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <signal.h>
#include <sys/wait.h>
#include <sys/socket.h>
#include <netdb.h>
#include <pthread.h>
#include <sodium.h>

// ── Linux Internal Types ─────────────────────────────────────
// These are Linux-specific. Not exposed in ABI.

typedef struct {
    int fd;
} linux_root_t;

typedef struct {
    int fd;
} linux_file_t;

typedef struct {
    DIR *dir;
} linux_dir_t;

typedef struct {
    int read_fd;
    int write_fd;
} linux_channel_t;

typedef struct {
    pid_t pid;
    int stdin_fd;
    int stdout_fd;
    int stderr_fd;
    bool waited;
} linux_process_t;

typedef struct {
    int fd;
} linux_socket_t;

typedef struct {
    int fd;
} linux_listener_t;

typedef struct {
    uint8_t private_key[crypto_sign_SECRETKEYBYTES];
    uint8_t public_key[32];
} linux_secret_t;

typedef struct {
    uint8_t public_key[crypto_sign_PUBLICKEYBYTES];
} linux_pubkey_t;

// ── Linux Implementation ─────────────────────────────────────

static int linux_init(void) {
    return 0;
}

static void linux_shutdown(void) {
    // Nothing to clean up globally
}

// ── Root ─────────────────────────────────────────────────────

static int64_t linux_root_open(const char *path) {
    if (!path) return -1;
    struct stat st;
    if (stat(path, &st) != 0) return -1;
    if (!S_ISDIR(st.st_mode)) return -1;

    int fd = open(path, O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (fd < 0) return -1;

    linux_root_t *root = pal_alloc(sizeof(linux_root_t));
    if (!root) { close(fd); return -1; }
    root->fd = fd;
    return (int64_t)root;
}

static void linux_root_close(int64_t internal) {
    linux_root_t *root = (linux_root_t *)internal;
    if (!root) return;
    if (root->fd >= 0) close(root->fd);
    pal_free(root);
}

// ── File ─────────────────────────────────────────────────────

static int64_t linux_file_open(int64_t root_internal, const char *rel_path, pal_open_flags flags) {
    linux_root_t *root = (linux_root_t *)root_internal;
    if (!root || !rel_path) return -1;

    int linux_flags = O_CLOEXEC;
    if (flags & PAL_OPEN_READ) linux_flags |= O_RDONLY;
    if (flags & PAL_OPEN_WRITE) linux_flags |= O_WRONLY;
    if (flags & PAL_OPEN_CREATE) linux_flags |= O_CREAT;
    if (flags & PAL_OPEN_TRUNCATE) linux_flags |= O_TRUNC;
    if (flags & PAL_OPEN_APPEND) linux_flags |= O_APPEND;

    int fd = openat(root->fd, rel_path, linux_flags, 0644);
    if (fd < 0) return -1;

    linux_file_t *file = pal_alloc(sizeof(linux_file_t));
    if (!file) { close(fd); return -1; }
    file->fd = fd;
    return (int64_t)file;
}

static int64_t linux_file_read(int64_t internal, void *buf, int64_t capacity) {
    linux_file_t *file = (linux_file_t *)internal;
    if (!file || !buf || capacity <= 0) return -1;
    ssize_t n = read(file->fd, buf, (size_t)capacity);
    if (n < 0) return -1;
    return (int64_t)n;
}

static int64_t linux_file_write(int64_t internal, const void *buf, int64_t length) {
    linux_file_t *file = (linux_file_t *)internal;
    if (!file || !buf || length <= 0) return -1;
    ssize_t n = write(file->fd, buf, (size_t)length);
    if (n < 0) return -1;
    return (int64_t)n;
}

static int64_t linux_file_seek(int64_t internal, int64_t offset, pal_seek_from_t from) {
    linux_file_t *file = (linux_file_t *)internal;
    if (!file) return -1;

    int posix_whence;
    switch (from) {
        case PAL_SEEK_BEGIN:   posix_whence = SEEK_SET; break;
        case PAL_SEEK_CURRENT: posix_whence = SEEK_CUR; break;
        case PAL_SEEK_END:     posix_whence = SEEK_END; break;
        default: return -1;
    }

    off_t r = lseek(file->fd, (off_t)offset, posix_whence);
    if (r < 0) return -1;
    return (int64_t)r;
}

static bool linux_file_stat(int64_t internal, pal_stat_t *out) {
    linux_file_t *file = (linux_file_t *)internal;
    if (!file || !out) return false;
    struct stat st;
    if (fstat(file->fd, &st) != 0) return false;
    out->size = (uint64_t)st.st_size;
    out->mode = (uint64_t)st.st_mode;
    out->mtime_ns = (int64_t)st.st_mtime * 1000000000;
    out->ctime_ns = (int64_t)st.st_ctime * 1000000000;
    out->dev = (uint64_t)st.st_dev;
    out->ino = (uint64_t)st.st_ino;
    return true;
}

static int64_t linux_file_size(int64_t internal) {
    linux_file_t *file = (linux_file_t *)internal;
    if (!file) return -1;
    struct stat st;
    if (fstat(file->fd, &st) != 0) return -1;
    return (int64_t)st.st_size;
}

static int64_t linux_file_clone(int64_t internal) {
    linux_file_t *file = (linux_file_t *)internal;
    if (!file) return -1;
    int dupfd = dup(file->fd);
    if (dupfd < 0) return -1;
    linux_file_t *clone = pal_alloc(sizeof(linux_file_t));
    if (!clone) { close(dupfd); return -1; }
    clone->fd = dupfd;
    return (int64_t)clone;
}

static void linux_file_close(int64_t internal) {
    linux_file_t *file = (linux_file_t *)internal;
    if (!file) return;
    if (file->fd >= 0) close(file->fd);
    pal_free(file);
}

// ── Directory ────────────────────────────────────────────────

static int64_t linux_dir_open(int64_t root_internal, const char *rel_path) {
    linux_root_t *root = (linux_root_t *)root_internal;
    if (!root || !rel_path) return -1;

    int fd = openat(root->fd, rel_path, O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (fd < 0) return -1;

    DIR *dir = fdopendir(fd);
    if (!dir) { close(fd); return -1; }

    linux_dir_t *d = pal_alloc(sizeof(linux_dir_t));
    if (!d) { closedir(dir); return -1; }
    d->dir = dir;
    return (int64_t)d;
}

static bool linux_dir_next(int64_t internal, pal_dir_entry_t *out) {
    linux_dir_t *d = (linux_dir_t *)internal;
    if (!d || !out) return false;
    struct dirent *de = readdir(d->dir);
    if (!de) return false;
    strncpy(out->name, de->d_name, sizeof(out->name) - 1);
    out->name[sizeof(out->name) - 1] = '\0';
    out->is_file = (de->d_type == DT_REG);
    out->is_dir = (de->d_type == DT_DIR);
    out->is_symlink = (de->d_type == DT_LNK);
    return true;
}

static void linux_dir_close(int64_t internal) {
    linux_dir_t *d = (linux_dir_t *)internal;
    if (!d) return;
    if (d->dir) closedir(d->dir);
    pal_free(d);
}

// ── Process ──────────────────────────────────────────────────

static int64_t linux_proc_create(const char **argv, pal_spawn_flags flags,
                                 int64_t stdin_internal, int64_t stdout_internal,
                                 int64_t stderr_internal) {
    if (!argv || !argv[0]) return -1;

    // Create pipe pairs for communication
    int stdin_pipe[2], stdout_pipe[2], stderr_pipe[2];
    if (pipe(stdin_pipe) != 0) return -1;
    if (pipe(stdout_pipe) != 0) { close(stdin_pipe[0]); close(stdin_pipe[1]); return -1; }
    if (pipe(stderr_pipe) != 0) {
        close(stdin_pipe[0]); close(stdin_pipe[1]);
        close(stdout_pipe[0]); close(stdout_pipe[1]);
        return -1;
    }

    pid_t pid = fork();
    if (pid < 0) {
        close(stdin_pipe[0]); close(stdin_pipe[1]);
        close(stdout_pipe[0]); close(stdout_pipe[1]);
        close(stderr_pipe[0]); close(stderr_pipe[1]);
        return -1;
    }

    if (pid == 0) {
        // Child: wire up pipes
        close(stdin_pipe[1]);   // Close write end
        close(stdout_pipe[0]);  // Close read end
        close(stderr_pipe[0]);  // Close read end

        dup2(stdin_pipe[0], STDIN_FILENO);
        dup2(stdout_pipe[1], STDOUT_FILENO);
        dup2(stderr_pipe[1], STDERR_FILENO);

        close(stdin_pipe[0]);
        close(stdout_pipe[1]);
        close(stderr_pipe[1]);

        execvp(argv[0], (char *const *)argv);
        _exit(127);
    }

    // Parent
    close(stdin_pipe[0]);   // Close read end
    close(stdout_pipe[1]);  // Close write end
    close(stderr_pipe[1]);  // Close write end

    linux_process_t *proc = pal_alloc(sizeof(linux_process_t));
    if (!proc) {
        close(stdin_pipe[1]);
        close(stdout_pipe[0]);
        close(stderr_pipe[0]);
        kill(pid, SIGKILL);
        waitpid(pid, NULL, 0);
        return -1;
    }
    proc->pid = pid;
    proc->stdin_fd = stdin_pipe[1];
    proc->stdout_fd = stdout_pipe[0];
    proc->stderr_fd = stderr_pipe[0];
    proc->waited = false;

    return (int64_t)proc;
}

static int64_t linux_proc_wait(int64_t internal) {
    linux_process_t *proc = (linux_process_t *)internal;
    if (!proc || proc->pid <= 0) return -1;
    int status;
    if (waitpid(proc->pid, &status, 0) < 0) return -1;
    proc->waited = true;
    if (WIFEXITED(status)) return WEXITSTATUS(status);
    return -1;
}

static bool linux_proc_kill(int64_t internal) {
    linux_process_t *proc = (linux_process_t *)internal;
    if (!proc || proc->pid <= 0 || proc->waited) return false;
    return kill(proc->pid, SIGTERM) == 0;
}

static int64_t linux_proc_channel(int fd, bool readable) {
    int duplicate = dup(fd);
    if (duplicate < 0) return -1;
    linux_channel_t *channel = pal_alloc(sizeof(*channel));
    if (!channel) {
        close(duplicate);
        return -1;
    }
    channel->read_fd = readable ? duplicate : -1;
    channel->write_fd = readable ? -1 : duplicate;
    return (int64_t)channel;
}

static int64_t linux_proc_stdin(int64_t internal) {
    linux_process_t *proc = (linux_process_t *)internal;
    return proc ? linux_proc_channel(proc->stdin_fd, false) : -1;
}

static int64_t linux_proc_stdout(int64_t internal) {
    linux_process_t *proc = (linux_process_t *)internal;
    return proc ? linux_proc_channel(proc->stdout_fd, true) : -1;
}

static int64_t linux_proc_stderr(int64_t internal) {
    linux_process_t *proc = (linux_process_t *)internal;
    return proc ? linux_proc_channel(proc->stderr_fd, true) : -1;
}

static void linux_proc_close(int64_t internal) {
    linux_process_t *proc = (linux_process_t *)internal;
    if (!proc) return;
    if (proc->pid > 0 && !proc->waited) {
        kill(proc->pid, SIGKILL);
        waitpid(proc->pid, NULL, 0);
    }
    if (proc->stdin_fd >= 0) close(proc->stdin_fd);
    if (proc->stdout_fd >= 0) close(proc->stdout_fd);
    if (proc->stderr_fd >= 0) close(proc->stderr_fd);
    pal_free(proc);
}

// ── Networking ───────────────────────────────────────────────

static int64_t linux_socket_connect(const char *host, uint16_t port, pal_socket_flags flags) {
    if (!host || (flags != PAL_SOCKET_TCP && flags != PAL_SOCKET_UDP)) return -1;
    struct addrinfo hints = {0};
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = flags == PAL_SOCKET_TCP ? SOCK_STREAM : SOCK_DGRAM;
    char service[6];
    snprintf(service, sizeof(service), "%u", port);
    struct addrinfo *results = NULL;
    if (getaddrinfo(host, service, &hints, &results) != 0) return -1;
    int fd = -1;
    for (struct addrinfo *it = results; it; it = it->ai_next) {
        fd = socket(it->ai_family, it->ai_socktype | SOCK_CLOEXEC, it->ai_protocol);
        if (fd < 0) continue;
        if (connect(fd, it->ai_addr, it->ai_addrlen) == 0) break;
        close(fd);
        fd = -1;
    }
    freeaddrinfo(results);
    if (fd < 0) return -1;
    linux_socket_t *socket_handle = pal_alloc(sizeof(*socket_handle));
    if (!socket_handle) {
        close(fd);
        return -1;
    }
    socket_handle->fd = fd;
    return (int64_t)socket_handle;
}

static int64_t linux_listener_bind(uint16_t port, pal_socket_flags flags) {
    if (flags != PAL_SOCKET_TCP && flags != PAL_SOCKET_UDP) return -1;
    struct addrinfo hints = {0};
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = flags == PAL_SOCKET_TCP ? SOCK_STREAM : SOCK_DGRAM;
    hints.ai_flags = AI_PASSIVE;
    char service[6];
    snprintf(service, sizeof(service), "%u", port);
    struct addrinfo *results = NULL;
    if (getaddrinfo(NULL, service, &hints, &results) != 0) return -1;
    int fd = -1;
    for (struct addrinfo *it = results; it; it = it->ai_next) {
        fd = socket(it->ai_family, it->ai_socktype | SOCK_CLOEXEC, it->ai_protocol);
        if (fd < 0) continue;
        int reuse = 1;
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));
        if (bind(fd, it->ai_addr, it->ai_addrlen) == 0 &&
            (flags == PAL_SOCKET_UDP || listen(fd, SOMAXCONN) == 0)) break;
        close(fd);
        fd = -1;
    }
    freeaddrinfo(results);
    if (fd < 0) return -1;
    linux_listener_t *listener = pal_alloc(sizeof(*listener));
    if (!listener) {
        close(fd);
        return -1;
    }
    listener->fd = fd;
    return (int64_t)listener;
}

static int64_t linux_listener_accept(int64_t listener_internal) {
    linux_listener_t *listener = (linux_listener_t *)listener_internal;
    if (!listener) return -1;
    int fd = accept(listener->fd, NULL, NULL);
    if (fd < 0) return -1;
    if (fcntl(fd, F_SETFD, FD_CLOEXEC) < 0) {
        close(fd);
        return -1;
    }
    linux_socket_t *socket_handle = pal_alloc(sizeof(*socket_handle));
    if (!socket_handle) {
        close(fd);
        return -1;
    }
    socket_handle->fd = fd;
    return (int64_t)socket_handle;
}

static int64_t linux_socket_send(int64_t internal, const void *buf, int64_t length) {
    linux_socket_t *sock = (linux_socket_t *)internal;
    if (!sock || !buf || length <= 0) return -1;
    ssize_t n = send(sock->fd, buf, (size_t)length, 0);
    if (n < 0) return -1;
    return (int64_t)n;
}

static int64_t linux_socket_recv(int64_t internal, void *buf, int64_t capacity) {
    linux_socket_t *sock = (linux_socket_t *)internal;
    if (!sock || !buf || capacity <= 0) return -1;
    ssize_t n = recv(sock->fd, buf, (size_t)capacity, 0);
    if (n < 0) return -1;
    return (int64_t)n;
}

static void linux_socket_close(int64_t internal) {
    linux_socket_t *sock = (linux_socket_t *)internal;
    if (!sock) return;
    if (sock->fd >= 0) close(sock->fd);
    pal_free(sock);
}

static void linux_listener_close(int64_t internal) {
    linux_listener_t *l = (linux_listener_t *)internal;
    if (!l) return;
    if (l->fd >= 0) close(l->fd);
    pal_free(l);
}

// ── Channels ─────────────────────────────────────────────────

static int64_t linux_channel_create(void) {
    int fds[2];
    if (pipe(fds) != 0) return -1;

    linux_channel_t *ch = pal_alloc(sizeof(linux_channel_t));
    if (!ch) { close(fds[0]); close(fds[1]); return -1; }
    ch->read_fd = fds[0];
    ch->write_fd = fds[1];
    return (int64_t)ch;
}

static int64_t linux_channel_send(int64_t internal, const void *buf, int64_t length) {
    linux_channel_t *ch = (linux_channel_t *)internal;
    if (!ch || !buf || length <= 0) return -1;
    ssize_t n = write(ch->write_fd, buf, (size_t)length);
    if (n < 0) return -1;
    return (int64_t)n;
}

static bool linux_channel_recv(int64_t internal, pal_bytes_t *out) {
    linux_channel_t *ch = (linux_channel_t *)internal;
    if (!ch || !out) return false;
    char buf[4096];
    ssize_t n = read(ch->read_fd, buf, sizeof(buf));
    if (n <= 0) return false;
    out->data = pal_alloc(n);
    if (!out->data) return false;
    memcpy(out->data, buf, (size_t)n);
    out->len = n;
    return true;
}

static void linux_channel_close(int64_t internal) {
    linux_channel_t *ch = (linux_channel_t *)internal;
    if (!ch) return;
    if (ch->read_fd >= 0) close(ch->read_fd);
    if (ch->write_fd >= 0) close(ch->write_fd);
    pal_free(ch);
}

// ── Crypto ──────────────────────────────────────────────────

static int64_t linux_secret_create(pal_crypto_algorithm_t algorithm) {
    if (algorithm != PAL_CRYPTO_ED25519 || sodium_init() < 0) return -1;
    linux_secret_t *secret = pal_secure_alloc(sizeof(*secret));
    if (!secret) return -1;
    if (crypto_sign_keypair(secret->public_key, secret->private_key) != 0) {
        pal_secure_free(secret);
        return -1;
    }
    return (int64_t)secret;
}

static int64_t linux_secret_export_public(int64_t secret_internal) {
    linux_secret_t *secret = (linux_secret_t *)secret_internal;
    if (!secret) return -1;
    linux_pubkey_t *public_key = pal_alloc(sizeof(*public_key));
    if (!public_key) return -1;
    memcpy(public_key->public_key, secret->public_key, sizeof(public_key->public_key));
    return (int64_t)public_key;
}

static int64_t linux_secret_sign(int64_t secret_internal, const void *msg, int64_t msg_len,
                                  void *buf, int64_t capacity) {
    linux_secret_t *secret = (linux_secret_t *)secret_internal;
    if (!secret || !msg || msg_len < 0 || !buf || capacity < crypto_sign_BYTES) return -1;
    unsigned long long signed_length = 0;
    if (crypto_sign_detached(buf, &signed_length, msg, (unsigned long long)msg_len,
                             secret->private_key) != 0) return -1;
    return (int64_t)signed_length;
}

static bool linux_pubkey_verify(int64_t pubkey_internal, const void *msg, int64_t msg_len,
                                 const void *sig, int64_t sig_len) {
    linux_pubkey_t *public_key = (linux_pubkey_t *)pubkey_internal;
    if (!public_key || !msg || msg_len < 0 || !sig || sig_len != crypto_sign_BYTES) return false;
    return crypto_sign_verify_detached(sig, msg, (unsigned long long)msg_len,
                                       public_key->public_key) == 0;
}

static void linux_secret_close(int64_t internal) {
    linux_secret_t *s = (linux_secret_t *)internal;
    if (!s) return;
    // Secure erase
    pal_secure_free(s);
}

static void linux_pubkey_close(int64_t internal) {
    pal_free((void *)internal);
}

// ── Stateless Services ───────────────────────────────────────

static int64_t linux_time_now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    return (int64_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}

static int64_t linux_time_now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    return (int64_t)(ts.tv_sec * 1000000000 + ts.tv_nsec);
}

static int64_t linux_cpu_count(void) {
    return (int64_t)sysconf(_SC_NPROCESSORS_ONLN);
}

static int64_t linux_mem_total(void) {
    FILE *f = fopen("/proc/meminfo", "r");
    if (!f) return 0;
    char line[256];
    int64_t total = 0;
    while (fgets(line, sizeof(line), f)) {
        if (strncmp(line, "MemTotal:", 9) == 0) {
            total = (int64_t)atoi(line + 9);
            break;
        }
    }
    fclose(f);
    return total * 1024;
}

static int64_t linux_mem_available(void) {
    FILE *f = fopen("/proc/meminfo", "r");
    if (!f) return 0;
    char line[256];
    int64_t avail = 0;
    while (fgets(line, sizeof(line), f)) {
        if (strncmp(line, "MemAvailable:", 12) == 0) {
            avail = (int64_t)atoi(line + 12);
            break;
        }
    }
    fclose(f);
    return avail * 1024;
}

static int64_t linux_mem_process(void) {
    FILE *f = fopen("/proc/self/status", "r");
    if (!f) return 0;
    char line[256];
    int64_t vm_rss = 0;
    while (fgets(line, sizeof(line), f)) {
        if (strncmp(line, "VmRSS:", 6) == 0) {
            vm_rss = (int64_t)atoi(line + 6);
            break;
        }
    }
    fclose(f);
    return vm_rss * 1024;
}

static bool linux_random_fill(void *buf, int64_t length) {
    if (!buf || length <= 0) return false;
    FILE *f = fopen("/dev/urandom", "r");
    if (!f) return false;
    size_t n = fread(buf, 1, (size_t)length, f);
    fclose(f);
    return n == (size_t)length;
}

// ── Backend Registration ─────────────────────────────────────

static const pal_ops_t linux_ops = {
    .init = linux_init,
    .shutdown = linux_shutdown,
    .root_open = linux_root_open,
    .root_close = linux_root_close,
    .file_open = linux_file_open,
    .file_read = linux_file_read,
    .file_write = linux_file_write,
    .file_seek = linux_file_seek,
    .file_stat = linux_file_stat,
    .file_size = linux_file_size,
    .file_clone = linux_file_clone,
    .file_close = linux_file_close,
    .dir_open = linux_dir_open,
    .dir_next = linux_dir_next,
    .dir_close = linux_dir_close,
    .proc_create = linux_proc_create,
    .proc_wait = linux_proc_wait,
    .proc_kill = linux_proc_kill,
    .proc_stdin = linux_proc_stdin,
    .proc_stdout = linux_proc_stdout,
    .proc_stderr = linux_proc_stderr,
    .proc_close = linux_proc_close,
    .socket_connect = linux_socket_connect,
    .listener_bind = linux_listener_bind,
    .listener_accept = linux_listener_accept,
    .socket_send = linux_socket_send,
    .socket_recv = linux_socket_recv,
    .socket_close = linux_socket_close,
    .listener_close = linux_listener_close,
    .channel_create = linux_channel_create,
    .channel_send = linux_channel_send,
    .channel_recv = linux_channel_recv,
    .channel_close = linux_channel_close,
    .secret_create = linux_secret_create,
    .secret_export_public = linux_secret_export_public,
    .secret_sign = linux_secret_sign,
    .pubkey_verify = linux_pubkey_verify,
    .secret_close = linux_secret_close,
    .pubkey_close = linux_pubkey_close,
    .time_now_ms = linux_time_now_ms,
    .time_now_ns = linux_time_now_ns,
    .cpu_count = linux_cpu_count,
    .mem_total = linux_mem_total,
    .mem_available = linux_mem_available,
    .mem_process = linux_mem_process,
    .random_fill = linux_random_fill,
};

__attribute__((constructor))
int pal_backend_register(void) {
    pal_dispatch_set_ops(&linux_ops);
    return pal_core_init(&linux_ops);
}

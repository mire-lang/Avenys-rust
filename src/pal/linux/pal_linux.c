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
#include <errno.h>
#include <sys/wait.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <netdb.h>
#include <pthread.h>
#include <sodium.h>
#include <sys/resource.h>
#include <time.h>

// ── openat2 sandbox helpers ─────────────────────────────
// openat2(2) was added in Linux 5.10. It allows RESOLVE_BENEATH
// which prevents path traversal outside the root directory.
// We define the structures and constants here so the code
// compiles on older kernels too; the syscall simply returns
// ENOSYS on kernels that don't support it, and we fall back
// to openat (which is why PAL_ALLOW_UNSANDBOXED exists).

#ifndef RESOLVE_BENEATH
#define RESOLVE_BENEATH    0x00000001
#define RESOLVE_NO_XDEV    0x00000002
#define RESOLVE_NO_SYMLINKS 0x00000004
#define RESOLVE_IN_ROOT    0x00000008
#endif

struct open_how {
    uint64_t flags;
    uint64_t mode;
    uint64_t resolve;
};

// openat2 at dirfd with a caller-chosen set of resolve flags.
// RESOLVE_NO_SYMLINKS alone rejects symlinks in intermediate components but
// permits mount-point crossings (RESOLVE_BENEATH returns EXDEV when a path
// crosses a mount, e.g. relative-to-"/" access to a tmpfs /tmp — see
// linux_root_remove). Absolute paths fall back to plain openat (legacy).
// ENOSYS (kernel < 5.10) falls back to openat. Returns -1 on error.
static int linux_openat2_resolve(int dirfd, const char *path, int flags, mode_t mode,
                                 uint64_t resolve) {
    if (!path || path[0] == '/') {
        return openat(dirfd, path, flags, mode);
    }
    struct open_how how = {
        .flags = (uint64_t)flags,
        .mode  = (uint64_t)mode,
        .resolve = resolve,
    };
    long ret = syscall(SYS_openat2, dirfd, path, &how, sizeof(how));
    if (ret == -1 && errno == ENOSYS) {
        return openat(dirfd, path, flags, mode);
    }
    return (int)ret;
}

// openat2 at dirfd with RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS.
// Relative paths are sandboxed against traversal (`..` and symlink
// escapes outside root are rejected). Absolute paths inherently ignore
// dirfd, so they fall back to plain openat (legacy semantics). Also
// falls back to openat on ENOSYS (kernel < 5.10). Returns -1 on error.
static int linux_openat2_sandbox(int dirfd, const char *path, int flags, mode_t mode) {
    return linux_openat2_resolve(dirfd, path, flags, mode,
                                 RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS);
}

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

// Capability-based single-entry removal relative to a root handle.
// Splits rel_path into parent + basename, resolves the parent inside the
// sandbox, then unlinks the basename without following it:
//   dir      -> unlinkat(dir_fd, base, AT_REMOVEDIR)  (PAL_ERR_NOT_EMPTY if full)
//   file/symlink/other -> unlinkat(dir_fd, base, 0)   (link unlinked, never followed)
// This fixes the prior bug of calling fstatat/unlinkat with an empty "name"
// on a bare fd: the parent directory is always resolved first.
// Parent resolution uses RESOLVE_NO_SYMLINKS (not RESOLVE_BENEATH): BENEATH
// returns EXDEV when the path crosses a mount point (relative-to-"/" access to
// a tmpfs /tmp), which would break removal of ordinary mount-backed paths. The
// anti-escape guarantee that matters for removal is that intermediate symlinks
// are never followed — provided by NO_SYMLINKS. "."/".." basenames are rejected
// below, and hosts compose only downward paths from the root handle.
static bool linux_root_remove(int64_t root_internal, const char *rel_path) {
    linux_root_t *root = (linux_root_t *)root_internal;
    if (!root || !rel_path || rel_path[0] == '\0') {
        pal_set_error(PAL_ERR_INVALID, "bad rel_path");
        return false;
    }

    size_t len = strlen(rel_path);
    while (len > 1 && rel_path[len - 1] == '/') len--; // "dir/" == "dir"
    if (len == 0) { pal_set_error(PAL_ERR_INVALID, "invalid rel_path"); return false; }

    size_t base_start = len;
    while (base_start > 0 && rel_path[base_start - 1] != '/') base_start--;
    const char *base = rel_path + base_start;
    size_t base_len = len - base_start;
    if (base_len == 0) { pal_set_error(PAL_ERR_INVALID, "invalid rel_path"); return false; }
    if (base_len == 1 && base[0] == '.') { pal_set_error(PAL_ERR_INVALID, "cannot remove ."); return false; }
    if (base_len == 2 && base[0] == '.' && base[1] == '.') { pal_set_error(PAL_ERR_INVALID, "cannot remove .."); return false; }

    int dir_fd;
    if (base_start == 0) {
        dir_fd = root->fd;
    } else {
        char parent[4096];
        if (base_start > sizeof(parent)) {
            pal_set_error(PAL_ERR_INVALID, "rel_path too long");
            return false;
        }
        memcpy(parent, rel_path, base_start - 1);
        parent[base_start - 1] = '\0';
        int fd = linux_openat2_resolve(root->fd, parent, O_RDONLY | O_DIRECTORY | O_CLOEXEC, 0,
                                       RESOLVE_NO_SYMLINKS);
        if (fd < 0) {
            pal_set_error(pal_core_errno_map(errno), "resolve parent");
            return false;
        }
        dir_fd = fd;
    }

    bool ok = false;
    struct stat st;
    if (fstatat(dir_fd, base, &st, AT_SYMLINK_NOFOLLOW) != 0) {
        pal_set_error(pal_core_errno_map(errno), "stat entry");
    } else if (S_ISDIR(st.st_mode)) {
        if (unlinkat(dir_fd, base, AT_REMOVEDIR) != 0) {
            pal_set_error(pal_core_errno_map(errno), "remove dir");
        } else {
            ok = true;
        }
    } else {
        // file, symlink, fifo, socket: unlink the entry itself, never follow it
        if (unlinkat(dir_fd, base, 0) != 0) {
            pal_set_error(pal_core_errno_map(errno), "unlink entry");
        } else {
            ok = true;
        }
    }

    if (dir_fd != root->fd) close(dir_fd);
    return ok;
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

    int fd = linux_openat2_sandbox(root->fd, rel_path, linux_flags, 0644);
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

    int fd = linux_openat2_sandbox(root->fd, rel_path, O_RDONLY | O_DIRECTORY | O_CLOEXEC, 0);
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
    if (de->d_type != DT_UNKNOWN) {
        out->is_file = (de->d_type == DT_REG);
        out->is_dir = (de->d_type == DT_DIR);
        out->is_symlink = (de->d_type == DT_LNK);
    } else {
        // Some filesystems (XFS, overlay, network FS) don't fill d_type.
        // Fall back to fstatat on the dirfd to determine the type.
        struct stat st;
        int ddirfd = dirfd(d->dir);
        if (ddirfd < 0 || fstatat(ddirfd, de->d_name, &st, AT_SYMLINK_NOFOLLOW) != 0) {
            out->is_file = false;
            out->is_dir = false;
            out->is_symlink = false;
        } else {
            out->is_file = S_ISREG(st.st_mode);
            out->is_dir = S_ISDIR(st.st_mode);
            out->is_symlink = S_ISLNK(st.st_mode);
        }
    }
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

    // Only create pipe pairs for channels the caller actually requested.
    // A 0 internal (PAL_CHANNEL_NULL) means "no pipe": the child inherits
    // the parent's fd for that stream instead of writing into a swallowed
    // pipe. Previously every stream was piped unconditionally, so
    // pal_proc_create(..., {0,0}, {0,0}, {0,0}) + pal_proc_wait silently
    // discarded the child's output (and could deadlock once >64KB wrote).
    int stdin_pipe[2] = {-1, -1}, stdout_pipe[2] = {-1, -1}, stderr_pipe[2] = {-1, -1};
    int has_stdin = stdin_internal != 0;
    int has_stdout = stdout_internal != 0;
    int has_stderr = stderr_internal != 0;
    if (has_stdin && pipe(stdin_pipe) != 0) return -1;
    if (has_stdout && pipe(stdout_pipe) != 0) {
        if (has_stdin) { close(stdin_pipe[0]); close(stdin_pipe[1]); }
        return -1;
    }
    if (has_stderr && pipe(stderr_pipe) != 0) {
        if (has_stdin) { close(stdin_pipe[0]); close(stdin_pipe[1]); }
        if (has_stdout) { close(stdout_pipe[0]); close(stdout_pipe[1]); }
        return -1;
    }

    pid_t pid = fork();
    if (pid < 0) {
        if (has_stdin) { close(stdin_pipe[0]); close(stdin_pipe[1]); }
        if (has_stdout) { close(stdout_pipe[0]); close(stdout_pipe[1]); }
        if (has_stderr) { close(stderr_pipe[0]); close(stderr_pipe[1]); }
        return -1;
    }

    if (pid == 0) {
        // Child: wire up pipes for requested streams, keep the inherited
        // parent fds for any stream the caller left as "no pipe".
        if (has_stdin) {
            close(stdin_pipe[1]);   // Close write end
            dup2(stdin_pipe[0], STDIN_FILENO);
            close(stdin_pipe[0]);
        }
        if (has_stdout) {
            close(stdout_pipe[0]);  // Close read end
            dup2(stdout_pipe[1], STDOUT_FILENO);
            close(stdout_pipe[1]);
        }
        if (has_stderr) {
            close(stderr_pipe[0]);  // Close read end
            dup2(stderr_pipe[1], STDERR_FILENO);
            close(stderr_pipe[1]);
        }

        execvp(argv[0], (char *const *)argv);
        _exit(127);
    }

    // Parent
    int stdin_fd = -1, stdout_fd = -1, stderr_fd = -1;
    if (has_stdin) { close(stdin_pipe[0]); stdin_fd = stdin_pipe[1]; }
    if (has_stdout) { close(stdout_pipe[1]); stdout_fd = stdout_pipe[0]; }
    if (has_stderr) { close(stderr_pipe[1]); stderr_fd = stderr_pipe[0]; }

    linux_process_t *proc = pal_alloc(sizeof(linux_process_t));
    if (!proc) {
        if (stdin_fd >= 0) close(stdin_fd);
        if (stdout_fd >= 0) close(stdout_fd);
        if (stderr_fd >= 0) close(stderr_fd);
        kill(pid, SIGKILL);
        waitpid(pid, NULL, 0);
        return -1;
    }
    proc->pid = pid;
    proc->stdin_fd = stdin_fd;
    proc->stdout_fd = stdout_fd;
    proc->stderr_fd = stderr_fd;
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

/* ─── Path / filesystem utility functions (PAL_ALLOW_UNSANDBOXED) ─── */

static const char *linux_fs_ext(const char *path) {
    const char *dot = strrchr(path, '.');
    if (!dot || dot == path) return strdup("");
    return strdup(dot + 1);
}

static const char *linux_fs_dir(const char *path) {
    char *dup = strdup(path);
    char *slash = strrchr(dup, '/');
    if (!slash) {
        free(dup);
        return strdup(".");
    }
    if (slash == dup) {
        slash[1] = '\0';
        return dup;
    }
    *slash = '\0';
    return dup;
}

static const char *linux_fs_name(const char *path) {
    const char *slash = strrchr(path, '/');
    if (!slash) return strdup(path);
    return strdup(slash + 1);
}

static bool linux_fs_is_file(const char *path) {
    struct stat st;
    if (stat(path, &st) != 0) return false;
    return S_ISREG(st.st_mode);
}

static bool linux_fs_copy(const char *src, const char *dst) {
    int fin = open(src, O_RDONLY);
    if (fin < 0) return false;
    int fout = open(dst, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fout < 0) { close(fin); return false; }
    char buf[8192];
    ssize_t n;
    while ((n = read(fin, buf, sizeof(buf))) > 0) {
        if (write(fout, buf, (size_t)n) != n) break;
    }
    close(fin);
    close(fout);
    if (n < 0) { unlink(dst); return false; }
    return true;
}

static bool linux_fs_move(const char *src, const char *dst) {
    return rename(src, dst) == 0;
}

/* ─── CPU / process timing ─── */

static int64_t linux_cpu_time_ms(void) {
    struct rusage ru;
    if (getrusage(RUSAGE_SELF, &ru) != 0) return pal_time_now_ms();
    int64_t user_ms = (int64_t)ru.ru_utime.tv_sec * 1000 + ru.ru_utime.tv_usec / 1000;
    int64_t sys_ms  = (int64_t)ru.ru_stime.tv_sec * 1000 + ru.ru_stime.tv_usec / 1000;
    return user_ms + sys_ms;
}

static const char *linux_cpu_snapshot(void) {
    struct rusage ru;
    if (getrusage(RUSAGE_SELF, &ru) != 0) return "";
    char buf[512];
    int64_t user_ms  = (int64_t)ru.ru_utime.tv_sec * 1000 + ru.ru_utime.tv_usec / 1000;
    int64_t sys_ms   = (int64_t)ru.ru_stime.tv_sec * 1000 + ru.ru_stime.tv_usec / 1000;
    int64_t total_ms = user_ms + sys_ms;
    int n = snprintf(buf, sizeof(buf),
        "{\"user_ms\":%lld,\"system_ms\":%lld,\"total_ms\":%lld,"
        "\"max_rss_kb\":%ld,\"voluntary_cs\":%ld,\"involuntary_cs\":%ld}",
        (long long)user_ms, (long long)sys_ms, (long long)total_ms,
        (long)ru.ru_maxrss, (long)ru.ru_nvcsw, (long)ru.ru_nivcsw);
    if (n < 0) return "";
    return strdup(buf);
}

/* ─── Memory utilities ─── */

static const char *linux_mem_format(int64_t bytes) {
    static const char *units[] = {"B", "KB", "MB", "GB", "TB", "PB"};
    int unit = 0;
    double sz = (double)bytes;
    while (sz >= 1024.0 && unit < 5) { sz /= 1024.0; unit++; }
    char buf[64];
    snprintf(buf, sizeof(buf), "%.1f %s", sz, units[unit]);
    return strdup(buf);
}

/* ─── Time snapshots (aliases / convenience) ─── */

static int64_t linux_time_mark(void) {
    return pal_time_now_ms();
}

static int64_t linux_time_unix_ms(void) {
    return pal_time_now_ms();
}

static int64_t linux_time_unix_ns(void) {
    return pal_time_now_ns();
}

/* ─── Environment ─── */

static const char *linux_env_all(void) {
    extern char **environ;
    size_t cap = 256;
    size_t len = 0;
    char *out = malloc(cap);
    if (!out) return "";
    out[0] = '\0';
    for (char **e = environ; *e; e++) {
        size_t elen = strlen(*e);
        if (len + elen + 2 > cap) {
            while (len + elen + 2 > cap) cap *= 2;
            out = realloc(out, cap);
        }
        memcpy(out + len, *e, elen);
        len += elen;
        out[len++] = '\n';
        out[len] = '\0';
    }
    return out;
}

/* ─── IO / diagnostics ─── */

static void linux_io_print_err(const char *msg) {
    if (msg) fputs(msg, stderr);
}

/* ─── Process management ─── */

static bool linux_proc_exists(int64_t pid) {
    return kill((pid_t)pid, 0) == 0;
}

static int64_t linux_proc_run(const char *cmd, const char **argv) {
    pid_t pid = fork();
    if (pid < 0) return -1;
    if (pid == 0) {
        execvp(cmd, (char *const *)argv);
        _exit(127);
    }
    int status;
    if (waitpid(pid, &status, 0) < 0) return -1;
    if (WIFEXITED(status)) return WEXITSTATUS(status);
    return -1;
}

/* ─── Legacy shell (PAL_ALLOW_LEGACY_SHELL) ─── */

#if PAL_ALLOW_LEGACY_SHELL
static int64_t linux_proc_system(const char *cmd) {
    return (int64_t)system(cmd);
}

static int64_t linux_proc_capture(const char *cmd, void *buf, int64_t capacity) {
    if (!buf || capacity <= 0) return -1;
    FILE *p = popen(cmd, "r");
    if (!p) return -1;
    size_t n = fread(buf, 1, (size_t)capacity, p);
    pclose(p);
    return (int64_t)n;
}

static const char *linux_proc_capture_output(const char *cmd) {
    FILE *p = popen(cmd, "r");
    if (!p) return NULL;
    char *buf = malloc(4096);
    size_t cap = 4096, n = 0;
    int c;
    while ((c = fgetc(p)) != EOF) {
        if (n + 1 >= cap) { cap *= 2; buf = realloc(buf, cap); }
        buf[n++] = (char)c;
    }
    pclose(p);
    buf[n] = '\0';
    return buf;
}
#endif

// ── Backend Registration ─────────────────────────────────────

static const pal_ops_t linux_ops = {
    .init = linux_init,
    .shutdown = linux_shutdown,
    .root_open = linux_root_open,
    .root_close = linux_root_close,
    .root_remove = linux_root_remove,
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
    .fs_ext = linux_fs_ext,
    .fs_dir = linux_fs_dir,
    .fs_name = linux_fs_name,
    .fs_is_file = linux_fs_is_file,
    .fs_copy = linux_fs_copy,
    .fs_move = linux_fs_move,
    .cpu_time_ms = linux_cpu_time_ms,
    .cpu_snapshot = linux_cpu_snapshot,
    .mem_format = linux_mem_format,
    .time_mark = linux_time_mark,
    .time_unix_ms = linux_time_unix_ms,
    .time_unix_ns = linux_time_unix_ns,
    .mem_process_bytes = linux_mem_process,
    .proc_exists = linux_proc_exists,
    .proc_run = linux_proc_run,
    .env_all = linux_env_all,
    .io_print_err = linux_io_print_err,
#if PAL_ALLOW_LEGACY_SHELL
    .proc_system = linux_proc_system,
    .proc_capture = linux_proc_capture,
    .proc_capture_output = linux_proc_capture_output,
#endif
};

__attribute__((constructor))
int pal_backend_register(void) {
    pal_dispatch_set_ops(&linux_ops);
    return pal_core_init(&linux_ops);
}

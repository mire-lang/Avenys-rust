#include "runtime.h"
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <poll.h>
#include <unistd.h>
#include <errno.h>
#include <string.h>

void *dasu(int64_t value) {
    printf("%ld\n", value);
    fflush(stdout);
    return NULL;
}

char *rt_io_read_line(void);

char *ireru(const char *prompt) {
    if (prompt && *prompt) {
        printf("%s", prompt);
        fflush(stdout);
    }
    // Shares the read buffer with the rest of the stdin API below. Reading this
    // one through stdio instead would split stdin's buffered bytes across two
    // owners: getchar() could swallow input that rt_io_available() then reports
    // as absent, which is the exact failure the buffer exists to prevent.
    return rt_io_read_line();
}

// ─────────────────────────────────────────────────────────────────────────────
// Standard streams: stdin (0), stdout (1), stderr (2).
//
// Everything here goes through the C stdio streams (fwrite/fread/fflush) rather
// than the raw file descriptors, deliberately. `dasu` and `ireru` above already
// use printf/getchar, and mixing a buffered FILE* with a raw write(2) on the
// same descriptor reorders output whenever the two disagree about how much has
// been flushed. fwrite is equally native and additionally binary safe: it copies
// a byte count and never stops at a NUL, unlike fputs.
//
// A Mire `str` may be a managed string or a plain literal, so lengths always go
// through rt_managed_len instead of strlen to stay correct for embedded NULs.
// ─────────────────────────────────────────────────────────────────────────────

// Resolve a stream selector to its FILE*. 1 is stdout, 2 is stderr; anything
// else clamps to stdout so a bad selector can never produce a null deref.
static FILE *io_stream(int64_t selector) {
    return (selector == 2) ? stderr : stdout;
}

// Write the whole string to a standard stream and flush it.
// Returns the number of bytes written, or -1 if the stream rejected the write.
int64_t rt_io_write(int64_t selector, const char *data) {
    return rt_io_write_n(selector, data, -1);
}

// Write at most `count` bytes of `data`. A negative count means "the whole
// string". Embedded NULs are written as ordinary bytes. Returns the byte count
// written, or -1 on error.
int64_t rt_io_write_n(int64_t selector, const char *data, int64_t count) {
    if (data == NULL) return -1;
    FILE *stream = io_stream(selector);
    size_t len = rt_managed_len(data);
    if (count >= 0 && (size_t)count < len) len = (size_t)count;
    if (len == 0) { fflush(stream); return 0; }
    size_t written = fwrite(data, 1, len, stream);
    fflush(stream);
    if (written != len) return -1;
    return (int64_t)written;
}

// Flush a standard stream. Returns 0 on success, -1 if the flush failed.
int64_t rt_io_flush(int64_t selector) {
    return fflush(io_stream(selector)) == 0 ? 0 : -1;
}

// Non-zero when the stream is in an error state, i.e. an earlier write failed.
int64_t rt_io_error(int64_t selector) {
    return ferror(io_stream(selector)) ? 1 : 0;
}

// Clear a stream's error and end-of-file indicators so the stream can be reused.
int64_t rt_io_clear(int64_t selector) {
    FILE *stream = io_stream(selector);
    clearerr(stream);
    return 0;
}

// The raw descriptor behind a stream: 1 for stdout, 2 for stderr. Exposed as an
// escape hatch for callers that must hand the fd to something else, such as
// pal_proc_create when wiring a child's output.
int64_t rt_io_fd(int64_t selector) {
    return (selector == 2) ? 2 : 1;
}

// ── stdin ────────────────────────────────────────────────────────────────────
//
// stdin is read through the raw descriptor with a buffer of our own, while
// stdout and stderr above go through stdio. The asymmetry is deliberate and is
// the whole reason available() can be trusted:
//
//   With stdio, getchar() pulls up to BUFSIZ bytes off the descriptor into a
//   buffer the rest of the program cannot see or measure. rt_io_available()
//   then has to poll the descriptor, which reports what the *kernel* has left,
//   not what the process already holds. Ask for the byte count after a line
//   read and the two disagree:
//
//       $ ( printf 'one\ntwo\n'; sleep 3 ) | ./prog
//       line()      -> "one"        (stdio now holds "two\n" in its buffer)
//       available() -> 0            (the pipe is open but empty; poll says idle)
//       all()       -> "two\n"      (4 bytes, returned immediately)
//
//   A program draining stdin with `while available() > 0` therefore drops
//   "two" silently, and one that blocks on available() can wait for input it
//   has already read. Owning the buffer removes the second source of truth:
//   available() is buffered + poll, and it is exact in both directions.
//
//   Nothing is lost by leaving stdio for writes. Reads have a single consumer,
//   so the interleaving hazard that motivates FILE* for output does not apply
//   to input.

#define MIRE_IO_READ_BUF 8192

static char g_read_buf[MIRE_IO_READ_BUF];
static size_t g_read_len = 0;  // bytes valid in g_read_buf
static size_t g_read_pos = 0;  // bytes already consumed from g_read_buf
static int g_read_eof = 0;     // descriptor reported end of input

// Bytes already in our buffer plus, when the buffer is empty, whatever poll says
// is waiting. Callers only need to know whether to block, never how much, so a
// single 0/1 answer is enough — but it must never under-report, or a drain loop
// silently truncates its input.
static int64_t io_input_available(void) {
    if (g_read_pos < g_read_len) return 1;
    if (g_read_eof) return 0;
    struct pollfd pfd;
    pfd.fd = 0;
    pfd.events = POLLIN;
    pfd.revents = 0;
    int rc = poll(&pfd, 1, 0);
    if (rc <= 0) return 0;
    return (pfd.revents & (POLLIN | POLLHUP)) ? 1 : 0;
}

// Pull the next chunk from the descriptor into the buffer. Returns the number
// of bytes now available, or 0 once input is exhausted.
static size_t io_read_fill(void) {
    if (g_read_pos < g_read_len) return g_read_len - g_read_pos;
    if (g_read_eof) return 0;
    g_read_pos = 0;
    g_read_len = 0;
    for (;;) {
        ssize_t got = read(0, g_read_buf, sizeof(g_read_buf));
        if (got > 0) {
            g_read_len = (size_t)got;
            return g_read_len;
        }
        if (got == 0) {  // clean end of input
            g_read_eof = 1;
            return 0;
        }
        if (errno == EINTR) continue;  // a signal is not end of input
        g_read_eof = 1;                // treat a hard error as end of input
        return 0;
    }
}

// Read one byte from stdin. Returns 0..255, or -1 at end of input.
int64_t rt_io_read_char(void) {
    if (io_read_fill() == 0) return -1;
    return (int64_t)(unsigned char)g_read_buf[g_read_pos++];
}

// Expose the unconsumed part of the buffer so bulk readers can work in chunks
// instead of one byte at a time. Returns 0 when input is exhausted.
static int io_read_peek(const char **out, size_t *out_len) {
    if (io_read_fill() == 0) {
        *out = NULL;
        *out_len = 0;
        return 0;
    }
    *out = g_read_buf + g_read_pos;
    *out_len = g_read_len - g_read_pos;
    return 1;
}

// Read one line from stdin and return it as a managed string with the trailing
// newline removed. Returns an empty string at end of input. The trailing CR of a
// CRLF pair is dropped as well, so input from any platform reads the same.
char *rt_io_read_line(void) {
    size_t cap = 128, len = 0;
    char *buf = (char *)malloc(cap);
    if (!buf) return rt_managed_from_slice("", 0);
    const char *chunk;
    size_t chunk_len;
    while (io_read_peek(&chunk, &chunk_len)) {
        const char *nl = (const char *)memchr(chunk, '\n', chunk_len);
        size_t take = nl ? (size_t)(nl - chunk) : chunk_len;
        if (len + take + 1 > cap) {
            while (len + take + 1 > cap) cap *= 2;
            char *nb = (char *)realloc(buf, cap);
            if (!nb) break;
            buf = nb;
        }
        memcpy(buf + len, chunk, take);
        len += take;
        g_read_pos += take + (nl ? 1 : 0);
        if (nl) break;
    }
    if (len > 0 && buf[len - 1] == '\r') len--;
    char *result = rt_managed_from_slice(buf, len);
    free(buf);
    return result;
}

// Read exactly `count` bytes from stdin, or fewer if the input ends first. The
// result is a managed string, so str::len reports how many bytes actually
// arrived. Binary safe: NUL bytes are preserved rather than terminating the
// read. Returns an empty string at end of input.
char *rt_io_read_bytes(int64_t count) {
    if (count <= 0) return rt_managed_from_slice("", 0);
    char *buf = (char *)malloc((size_t)count);
    if (!buf) return rt_managed_from_slice("", 0);
    size_t total = 0;
    while (total < (size_t)count) {
        const char *chunk;
        size_t chunk_len;
        if (!io_read_peek(&chunk, &chunk_len)) break;
        size_t take = chunk_len;
        if (take > (size_t)count - total) take = (size_t)count - total;
        memcpy(buf + total, chunk, take);
        total += take;
        g_read_pos += take;
    }
    char *result = rt_managed_from_slice(buf, total);
    free(buf);
    return result;
}

// Read stdin to end of input and return it as a managed string. Binary safe.
// Returns an empty string when stdin is already at end of input.
char *rt_io_read_all(void) {
    size_t cap = 8192, len = 0;
    char *buf = (char *)malloc(cap);
    if (!buf) return rt_managed_from_slice("", 0);
    for (;;) {
        const char *chunk;
        size_t chunk_len;
        if (!io_read_peek(&chunk, &chunk_len)) break;
        if (len + chunk_len + 1 > cap) {
            while (len + chunk_len + 1 > cap) cap *= 2;
            char *nb = (char *)realloc(buf, cap);
            if (!nb) break;
            buf = nb;
        }
        memcpy(buf + len, chunk, chunk_len);
        len += chunk_len;
        g_read_pos += chunk_len;
    }
    char *result = rt_managed_from_slice(buf, len);
    free(buf);
    return result;
}

// Bytes waiting on stdin without blocking, or 0 when nothing is ready. Counts
// what this process has already buffered as well as what the descriptor still
// holds, so it never under-reports.
int64_t rt_io_available(void) {
    return io_input_available();
}

// Non-zero when stdin is an interactive terminal. A program that prompts should
// check this so it can pick between an interactive read and a non-blocking one.
int64_t rt_io_is_tty(void) {
    return isatty(0) ? 1 : 0;
}

// Write text followed by a newline in a single write, so a line is not split
// across two buffer flushes and cannot be interleaved with another writer's
// output. Returns the total number of bytes written, or -1 on error.
int64_t rt_io_write_line(int64_t selector, const char *data) {
    if (data == NULL) return -1;
    FILE *stream = io_stream(selector);
    size_t len = rt_managed_len(data);
    if (fwrite(data, 1, len, stream) != len) return -1;
    if (fputc('\n', stream) == EOF) return -1;
    fflush(stream);
    return (int64_t)(len + 1);
}

// Write an integer, optionally followed by a newline. Formats into a stack
// buffer and writes the bytes directly, so printing a number never allocates a
// managed string. `newline` is 0 to suppress the terminator. Returns the number
// of bytes written, or -1 on error.
int64_t rt_io_write_i64(int64_t selector, int64_t value, int64_t newline) {
    char buf[32];
    int n = snprintf(buf, sizeof(buf), "%lld", (long long)value);
    if (n < 0) return -1;
    size_t len = (size_t)n;
    if (newline && len < sizeof(buf)) buf[len++] = '\n';
    FILE *stream = io_stream(selector);
    size_t written = fwrite(buf, 1, len, stream);
    fflush(stream);
    return written == len ? (int64_t)len : -1;
}

// Write a float using %g, which is the shortest representation that round-trips
// and therefore the right default for diagnostics. `newline` behaves as in
// rt_io_write_i64. Returns the number of bytes written, or -1 on error.
int64_t rt_io_write_f64(int64_t selector, double value, int64_t newline) {
    char buf[64];
    int n = snprintf(buf, sizeof(buf), "%g", value);
    if (n < 0) return -1;
    size_t len = (size_t)n;
    if (newline && len < sizeof(buf)) buf[len++] = '\n';
    FILE *stream = io_stream(selector);
    size_t written = fwrite(buf, 1, len, stream);
    fflush(stream);
    return written == len ? (int64_t)len : -1;
}

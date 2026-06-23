#ifndef MIRE_PAL_H
#define MIRE_PAL_H

#include <stddef.h>
#include <stdint.h>

// PAL — Platform Abstraction Layer
// Each domain groups system calls behind a stable API.
// Platform backends live in pal/<platform>/ directories.

// ── Filesystem ───────────────────────────────────────────────────────
int     pal_fs_write(const char *path, const char *content);
int     pal_fs_append(const char *path, const char *content);
char   *pal_fs_read(const char *path);
int     pal_fs_copy(const char *src, const char *dst);
int     pal_fs_move(const char *src, const char *dst);
int     pal_fs_delete(const char *path);
int     pal_fs_mkdir(const char *path);
int     pal_fs_rmdir(const char *path);
int64_t pal_fs_exists(const char *path);
int64_t pal_fs_is_dir(const char *path);
int64_t pal_fs_is_file(const char *path);
int64_t pal_fs_size(const char *path);
void   *pal_fs_list(const char *path);
char   *pal_fs_join(const char *a, const char *b);
char   *pal_fs_dir(const char *path);
char   *pal_fs_name(const char *path);
char   *pal_fs_ext(const char *path);

// ── Environment ──────────────────────────────────────────────────────
char   *pal_env_get(const char *name);
int     pal_env_set(const char *name, const char *value);
void   *pal_env_all(void);
char   *pal_env_cwd(void);
int     pal_env_chdir(const char *path);

// ── Process ──────────────────────────────────────────────────────────
char   *pal_proc_run(const char *cmd);
char   *pal_proc_exec(const char *cmd);
char   *pal_proc_shell(const char *cmd);
int64_t pal_proc_spawn(const char *cmd);
int64_t pal_proc_wait(int64_t pid);
int     pal_proc_kill(int64_t pid);
void    pal_proc_exit(int64_t status);
int64_t pal_proc_exists(int64_t pid);
char   *pal_proc_err(void);
void    pal_proc_on(const char *signal_name);

// ── Time ─────────────────────────────────────────────────────────────
int64_t pal_time_unix_ms(void);
int64_t pal_time_unix_ns(void);
int64_t pal_time_since_ms(int64_t start_ns);
int64_t pal_time_since_ns(int64_t start_ns);
void    pal_time_sleep_ms(int64_t ms);
void    pal_time_sleep_ns(int64_t ns);
int64_t pal_time_mark(void);
int64_t pal_time_elapsed_ms(int64_t start_ns);
int64_t pal_time_elapsed_ns(int64_t start_ns);

// ── CPU ──────────────────────────────────────────────────────────────
int64_t pal_cpu_time_ns(void);
int64_t pal_cpu_time_ms(void);
int64_t pal_cpu_mark(void);
int64_t pal_cpu_elapsed_ms(int64_t start_ns);
int64_t pal_cpu_elapsed_ns(int64_t start_ns);
int64_t pal_cpu_count(void);
int64_t pal_cpu_freq_mhz(void);
int64_t pal_cpu_cycles_est(int64_t start_ns);
void   *pal_cpu_loadavg(void);
void   *pal_cpu_snapshot(void);

// ── Memory ───────────────────────────────────────────────────────────
int64_t pal_mem_used(void);
int64_t pal_mem_total(void);
int64_t pal_mem_free(void);
int64_t pal_mem_available(void);
int64_t pal_mem_percent(void);
int64_t pal_mem_process_bytes(void);
void   *pal_mem_snapshot(void);
char   *pal_mem_format(int64_t bytes);

// ── GPU ──────────────────────────────────────────────────────────────
char   *pal_gpu_snapshot(void);

// ── Terminal ─────────────────────────────────────────────────────────
char   *pal_term_style(const char *text, const char *style);
char   *pal_term_hr(const char *ch, int64_t len);
char   *pal_term_clear(void);

// ── WebSocket ────────────────────────────────────────────────────────
int64_t pal_ws_connect(const char *host, int64_t port, const char *path);
int     pal_ws_send_text(int64_t fd, const char *data);
char   *pal_ws_recv(int64_t fd, int64_t max_bytes);
int     pal_ws_close(int64_t fd);
int64_t pal_wss_connect(const char *host, int64_t port, const char *path);
int     pal_wss_send_text(int64_t fd, const char *data);
char   *pal_wss_recv(int64_t fd, int64_t max_bytes);
int     pal_wss_close(int64_t fd);

// ── Networking ───────────────────────────────────────────────────────
int64_t pal_net_connect(const char *host, int64_t port);
int64_t pal_net_connect_timeout(const char *host, int64_t port, int64_t timeout_ms);
char   *pal_net_recv(int64_t fd, int64_t max_bytes);
int     pal_net_send(int64_t fd, const char *data);
int     pal_net_close(int64_t fd);
int64_t pal_net_poll(int64_t fd, int64_t timeout_ms);
int     pal_net_set_nonblock(int64_t fd, int nonblock);
char   *pal_net_resolve(const char *host);

// ── TLS / SSL (OpenSSL) ──────────────────────────────────────────────
int64_t pal_tls_connect(const char *host, int64_t port);
int     pal_tls_send(int64_t fd, const char *data);
char   *pal_tls_recv(int64_t fd, int64_t max_bytes);
int     pal_tls_close(int64_t fd);

// ── I/O helpers ──────────────────────────────────────────────────────
void    pal_io_print(const char *msg);
void    pal_io_print_err(const char *msg);
char   *pal_io_readln(void);

#endif // MIRE_PAL_H

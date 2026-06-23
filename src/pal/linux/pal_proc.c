#include "../pal.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>
#include <signal.h>

// ── Captured stderr from last proc.shell / proc.run ───────────────────
static char *pal_last_stderr = NULL;

static void pal_free_stderr(void) {
    if (pal_last_stderr) { free(pal_last_stderr); pal_last_stderr = NULL; }
}

// Read all output from a FILE* into a malloc'd string.
static char *read_all(FILE *fh) {
    if (!fh) return NULL;
    size_t cap = 256, len = 0;
    char *buf = (char *)malloc(cap);
    if (!buf) { fclose(fh); return NULL; }
    int ch;
    while ((ch = fgetc(fh)) != EOF) {
        if (len + 1 >= cap) {
            cap *= 2;
            char *nb = (char *)realloc(buf, cap);
            if (!nb) break;
            buf = nb;
        }
        buf[len++] = (char)ch;
    }
    buf[len] = '\0';
    fclose(fh);
    return buf;
}

char *pal_proc_run(const char *cmd) {
    FILE *fh = popen(cmd, "r");
    if (!fh) return NULL;
    return read_all(fh);
}

// Run cmd, capturing both stdout and stderr.
// Returns stdout; stderr is stored in pal_last_stderr.
static char *run_captured(const char *cmd) {
    pal_free_stderr();
    char tmpl[] = "/tmp/mire_stderr_XXXXXX";
    int fd = mkstemp(tmpl);
    if (fd < 0) return pal_proc_run(cmd);
    close(fd);

    size_t cmdlen = strlen(cmd);
    size_t redirlen = strlen(tmpl);
    char *full_cmd = (char *)malloc(cmdlen + redirlen + 32);
    if (!full_cmd) { unlink(tmpl); return pal_proc_run(cmd); }
    snprintf(full_cmd, cmdlen + redirlen + 32, "%s 2>%s", cmd, tmpl);

    FILE *fh = popen(full_cmd, "r");
    free(full_cmd);
    if (!fh) { unlink(tmpl); return NULL; }
    char *out = read_all(fh);
    FILE *efh = fopen(tmpl, "r");
    if (efh) {
        pal_last_stderr = read_all(efh);
    }
    unlink(tmpl);
    return out;
}

char *pal_proc_exec(const char *cmd) {
    return run_captured(cmd);
}

char *pal_proc_shell(const char *cmd) {
    return run_captured(cmd);
}

char *pal_proc_err(void) {
    return pal_last_stderr ? strdup(pal_last_stderr) : strdup("");
}

int64_t pal_proc_spawn(const char *cmd) {
    pid_t pid = fork();
    if (pid < 0) return -1;
    if (pid == 0) {
        execl("/bin/sh", "sh", "-c", cmd ? cmd : "", (char *)NULL);
        _exit(127);
    }
    return (int64_t)pid;
}

int64_t pal_proc_wait(int64_t pid) {
    int status;
    if (waitpid((pid_t)pid, &status, 0) < 0) return -1;
    if (WIFEXITED(status)) return WEXITSTATUS(status);
    return -1;
}

int pal_proc_kill(int64_t pid) {
    return kill((pid_t)pid, SIGTERM) == 0 ? 1 : 0;
}

void pal_proc_exit(int64_t status) {
    exit((int)status);
}

int64_t pal_proc_exists(int64_t pid) {
    return kill((pid_t)pid, 0) == 0 ? 1 : 0;
}

// ── Signal handling ───────────────────────────────────────────────────
static volatile int pal_last_signal = 0;

static void pal_signal_handler(int sig) {
    pal_last_signal = sig;
}

void pal_proc_on(const char *signal_name) {
    int sig = 0;
    if (strcmp(signal_name, "INT") == 0 || strcmp(signal_name, "SIGINT") == 0) sig = SIGINT;
    else if (strcmp(signal_name, "TERM") == 0 || strcmp(signal_name, "SIGTERM") == 0) sig = SIGTERM;
    else if (strcmp(signal_name, "HUP") == 0 || strcmp(signal_name, "SIGHUP") == 0) sig = SIGHUP;
    else if (strcmp(signal_name, "QUIT") == 0 || strcmp(signal_name, "SIGQUIT") == 0) sig = SIGQUIT;
    else if (strcmp(signal_name, "USR1") == 0 || strcmp(signal_name, "SIGUSR1") == 0) sig = SIGUSR1;
    else if (strcmp(signal_name, "USR2") == 0 || strcmp(signal_name, "SIGUSR2") == 0) sig = SIGUSR2;
    else if (strcmp(signal_name, "CHLD") == 0 || strcmp(signal_name, "SIGCHLD") == 0) sig = SIGCHLD;
    if (sig) signal(sig, pal_signal_handler);
}

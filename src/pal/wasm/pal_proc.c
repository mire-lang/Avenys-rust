// WASM PAL — Process stubs
// WASM has no process creation or management.

#include "pal.h"
#include <stdlib.h>

char *pal_proc_run(const char *cmd) { (void)cmd; return NULL; }
char *pal_proc_exec(const char *cmd) { (void)cmd; return NULL; }
char *pal_proc_shell(const char *cmd) { (void)cmd; return NULL; }
int64_t pal_proc_spawn(const char *cmd) { (void)cmd; return -1; }
int64_t pal_proc_spawn_argv(const char **argv) { (void)argv; return -1; }
int64_t pal_proc_wait(int64_t pid) { (void)pid; return -1; }
int pal_proc_kill(int64_t pid) { (void)pid; return -1; }
int64_t pal_proc_exists(int64_t pid) { (void)pid; return 0; }
int pal_proc_last_signal(void) { return 0; }

void pal_proc_exit(int64_t status) {
    __builtin_trap();
}

void pal_proc_on(const char *signal_name) {
    (void)signal_name;
    // Signal handlers are not supported under WASI.
}

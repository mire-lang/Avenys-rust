// PAL v4 — Sandbox policy engine
//
// A single, explicit policy that every absolute-path filesystem entry point
// consults before it reaches the kernel. The point of putting it here, in
// core, rather than in the language runtime is that a policy enforced by the
// language layer is advisory: a program can always declare its own extern and
// call the kernel anyway. Enforcement at the syscall boundary is the only
// version of this that is a security control rather than a lint.
//
// ── The model ──
//
//   * A sandbox ROOT: an absolute, normalised prefix. Default is the process
//     working directory, captured lazily on first use. `NULL` means "deny
//     every absolute path".
//   * An UNSAFE DEPTH: a per-thread counter. While it is greater than zero the
//     policy is suspended. This is the escape hatch, and it is deliberately
//     not a boolean: a nested scope that forgets to exit must not silently
//     widen the hole, so enter/exit have to balance.
//
// ── What the check actually does ──
//
// Three stages, because a prefix test alone is trivially defeated:
//
//   1. LEXICAL. Join with the root if relative, then normalise `.` and `..`
//      textually. An underflow past `/` is a denial, so `/root/../etc` is
//      rejected on the text, not on the filesystem.
//   2. PREFIX. The normalised result must be the root or sit under `root/`.
//   3. SYMLINK. The deepest EXISTING ancestor is resolved with realpath(3) and
//      the prefix test is repeated. A link inside the root that points at
//      `/etc` is rejected, which stage 1 cannot see.
//
// Stage 3 needs the "deepest existing ancestor" trick because realpath(3)
// fails outright on a path that does not exist yet, and creating a file
// through a symlinked parent is exactly the case worth catching.
//
// ── What this is not ──
//
// This is a policy on PATH, not a jail. It does not stop a process that has
// already been handed a descriptor, it does not constrain the network, and it
// is not a substitute for running with dropped privileges. For a tree you did
// not build, the capability API (pal_root_open + pal_file_open) is stronger
// still, because it resolves with RESOLVE_BENEATH in the kernel. Use both.

// realpath(3) is POSIX, not ISO C, so a strict -std=c11 build hides it behind
// a feature-test macro. Feature-test macros only affect headers included
// AFTER them, so this has to sit above every include — including pal_core.h.
#ifndef _DEFAULT_SOURCE
#define _DEFAULT_SOURCE 1
#endif
#ifndef _POSIX_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#endif

#include "pal_core.h"

#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

// POSIX allows PATH_MAX to be absent on a system that has no fixed limit; the
// buffers below are sized by it, so fall back to the historical value rather
// than failing to compile.
#ifndef PATH_MAX
#define PATH_MAX 4096
#endif

// ── State ──
// The root is a property of the PROCESS: it is policy, not a call-site
// override, and two threads disagreeing about what the sandbox is would be a
// bug waiting to happen. The depth is per-THREAD, because an unsafe scope is
// a lexical property of the code that opened it.
static char g_sbx_root[PATH_MAX];
static bool g_sbx_root_set = false;

// A second, separately authorised subtree: the tool's own home (~/.owl).
//
// Why this exists and why it is narrow. The build orchestrator has to keep a
// package store somewhere that outlives any one project, and a project tree is
// the wrong place: it cannot be shared, and copying a dependency per checkout
// defeats the point of having a registry. But the sandbox's answer to "a program
// wants to write outside the project" is a deliberate no, and widening the main
// root to include the home directory would hand every program in every project
// write access to everything in it.
//
// So the home is authorised SEPARATELY, not folded into the root:
//
//   * Only paths under this one directory pass. Everything else is unaffected.
//   * Reads are allowed, writes are allowed, because a package store is
//     meaningless otherwise.
//   * It grants no execution. Nothing here can be run, spawned or linked as
//     code: the sandbox constrains paths, and the capability API plus the
//     separate process gates are what constrain execution. A file dropped into
//     the store by an attacker is inert data that owl will refuse to install
//     unless it matches the registry's hash.
//   * It is set explicitly by the tool that owns it, never inferred from the
//     environment, so a project cannot widen its own sandbox by exporting a
//     variable.
//
// Two roots is the whole change; the three-stage check below is reused verbatim
// for both, so traversal and symlink escapes are rejected here exactly as they
// are in the project root.
static char g_sbx_tool_home[PATH_MAX];
static bool g_sbx_tool_home_set = false;

static MIRE_TLS int t_sbx_unsafe = 0;
static MIRE_TLS pal_sbx_reason_t t_sbx_reason = PAL_SBX_DENY_NONE;
static MIRE_TLS char t_sbx_detail[256] = {0};

// ── Root ──

static void sbx_capture_cwd_root(void) {
    if (g_sbx_root_set) return;
    if (getcwd(g_sbx_root, sizeof(g_sbx_root)) == NULL) {
        // No working directory to speak of. An empty root denies everything
        // absolute, which is the correct failure direction: fail closed.
        g_sbx_root[0] = '\0';
    }
    g_sbx_root_set = true;
}

void pal_sbx_set_root(const char *root) {
    if (!root || root[0] == '\0') {
        g_sbx_root[0] = '\0';
        g_sbx_root_set = true;
        return;
    }
    char resolved[PATH_MAX];
    if (realpath(root, resolved) == NULL) {
        // A root that does not exist cannot contain anything. Record it as an
        // empty root rather than the caller's string, so a typo denies rather
        // than accidentally widening to "/".
        g_sbx_root[0] = '\0';
        g_sbx_root_set = true;
        return;
    }
    size_t n = strlen(resolved);
    if (n > 1 && resolved[n - 1] == '/') resolved[n - 1] = '\0'; // strip trailing /
    strncpy(g_sbx_root, resolved, sizeof(g_sbx_root) - 1);
    g_sbx_root[sizeof(g_sbx_root) - 1] = '\0';
    g_sbx_root_set = true;
}

const char *pal_sbx_get_root(void) {
    sbx_capture_cwd_root();
    return g_sbx_root[0] == '\0' ? NULL : g_sbx_root;
}

// Authorise a second subtree (the tool's own home) in addition to the project
// root. Pass NULL to revoke it, which is the safe default: a program that has not
// asked for it does not get it.
//
// realpath(3) is applied for the same reason as the main root: a string that
// does not resolve is a string that cannot contain anything, and recording it
// verbatim would let a typo widen the hole instead of closing it.
void pal_sbx_set_tool_home(const char *path) {
    if (!path || path[0] == '\0') {
        g_sbx_tool_home[0] = '\0';
        g_sbx_tool_home_set = true;
        return;
    }
    // The directory may not exist yet on a first run. Resolve the deepest
    // existing ancestor and re-attach the remainder, so a fresh install is not
    // refused just because nothing has been written there yet.
    char resolved[PATH_MAX];
    if (realpath(path, resolved) != NULL) {
        size_t n = strlen(resolved);
        if (n > 1 && resolved[n - 1] == '/') resolved[n - 1] = '\0';
        strncpy(g_sbx_tool_home, resolved, sizeof(g_sbx_tool_home) - 1);
        g_sbx_tool_home[sizeof(g_sbx_tool_home) - 1] = '\0';
        g_sbx_tool_home_set = true;
        return;
    }

    char probe[PATH_MAX];
    strncpy(probe, path, sizeof(probe) - 1);
    probe[sizeof(probe) - 1] = '\0';
    size_t plen = strlen(probe);
    while (plen > 1 && probe[plen - 1] != '/') probe[--plen] = '\0';
    if (realpath(probe, resolved) == NULL) {
        g_sbx_tool_home[0] = '\0';
        g_sbx_tool_home_set = true;
        return;
    }
    size_t rlen = strlen(resolved);
    if (rlen > 1 && resolved[rlen - 1] == '/') resolved[rlen - 1] = '\0';
    rlen = strlen(resolved);
    if (rlen >= sizeof(g_sbx_tool_home)) {
        g_sbx_tool_home[0] = '\0';
        g_sbx_tool_home_set = true;
        return;
    }
    memcpy(g_sbx_tool_home, resolved, rlen + 1);
    g_sbx_tool_home_set = true;
}

const char *pal_sbx_get_tool_home(void) {
    return g_sbx_tool_home_set ? (g_sbx_tool_home[0] == '\0' ? NULL : g_sbx_tool_home) : NULL;
}

// ── Unsafe depth ──

bool pal_sbx_set_unsafe(int depth) {
    if (depth < 0) depth = 0;
    t_sbx_unsafe = depth;
    return true;
}

int pal_sbx_unsafe_depth(void) { return t_sbx_unsafe; }

// ── Denial bookkeeping ──

static bool sbx_deny(pal_sbx_reason_t reason, const char *fmt, const char *arg) {
    t_sbx_reason = reason;
    if (arg) {
        snprintf(t_sbx_detail, sizeof(t_sbx_detail), fmt, arg);
    } else {
        snprintf(t_sbx_detail, sizeof(t_sbx_detail), "%s", fmt);
    }
    return false;
}

pal_sbx_reason_t pal_sbx_reason(void) { return t_sbx_reason; }

const char *pal_sbx_reason_text(void) {
    switch (t_sbx_reason) {
        case PAL_SBX_DENY_NONE:      return "allowed";
        case PAL_SBX_DENY_NO_ROOT:   return "sandbox has no root; every absolute path is denied";
        case PAL_SBX_DENY_ABSOLUTE:  return "absolute path is outside the sandbox root";
        case PAL_SBX_DENY_ESCAPE:    return "path escapes the sandbox root via ..";
        case PAL_SBX_DENY_TRAVERSAL: return "path escapes the filesystem root";
        case PAL_SBX_DENY_SYMLINK:   return "path resolves through a symlink out of the sandbox";
    }
    return "unknown";
}

const char *pal_sbx_detail(void) { return t_sbx_detail; }

// ── Lexical normalisation ──
// Collapses `//`, drops `.`, pops on `..`. Returns false if a `..` underflows
// above the root of the filesystem, which is a denial in its own right.
// `saw_dotdot` reports whether the input actually contained a `..`: a caller
// that *tried* to climb out deserves PAL_SBX_DENY_ESCAPE rather than the
// generic "outside the root", because "was this an escape attempt" is the
// question worth answering in a log.
static bool sbx_normalize(const char *in, char *out, size_t out_size, bool *saw_dotdot) {
    *saw_dotdot = false;
    size_t o = 0;
    out[o++] = '/';
    if (in[0] == '/') in++;

    while (*in) {
        while (*in == '/') in++;
        if (*in == '\0') break;

        const char *seg = in;
        while (*in && *in != '/') in++;
        size_t seg_len = (size_t)(in - seg);

        if (seg_len == 1 && seg[0] == '.') continue;

        if (seg_len == 2 && seg[0] == '.' && seg[1] == '.') {
            *saw_dotdot = true;
            if (o <= 1) return false; // underflow: climbed above "/"
            while (o > 1 && out[o - 1] != '/') o--;
            if (o > 1) o--; // drop the separator too
            if (o < 1) o = 1;
            out[o] = '\0';
            continue;
        }

        if (o > 1) {
            if (o + 1 >= out_size) return false;
            out[o++] = '/';
        }
        if (o + seg_len >= out_size) return false;
        memcpy(out + o, seg, seg_len);
        o += seg_len;
        out[o] = '\0';
    }
    out[o] = '\0';
    return true;
}

// ── Prefix test ──
static bool sbx_under_root(const char *resolved, const char *root) {
    size_t rlen = strlen(root);
    if (rlen == 0) return false;
    if (strncmp(resolved, root, rlen) != 0) return false;
    return resolved[rlen] == '\0' || resolved[rlen] == '/';
}

// ── Symlink-aware re-check ──
// Resolves the longest existing prefix of `path` with realpath(3) and re-applies
// the prefix test to `resolved_prefix + remaining`. Returns true when the
// result is outside the root (i.e. a denial); false when it is fine.
static bool sbx_symlink_escapes(const char *lexical, const char *root) {
    char probe[PATH_MAX];
    snprintf(probe, sizeof(probe), "%s", lexical);

    // Walk up until something exists. "/a/b/c" -> "/a/b" -> "/a" -> "/".
    for (;;) {
        char resolved[PATH_MAX];
        if (realpath(probe, resolved) != NULL) {
            if (!sbx_under_root(resolved, root)) return true; // escapes
            // The existing prefix is inside the root. The rest of the path is
            // new names under it, so it cannot leave.
            return false;
        }
        if (strcmp(probe, "/") == 0) return false; // reached "/" and it's the root
        char *slash = strrchr(probe, '/');
        if (!slash) return false;
        if (slash == probe) {
            probe[1] = '\0'; // truncate to "/"
        } else {
            *slash = '\0';
        }
    }
}

// ── The policy ──

// Why one candidate root rejected a path. The two roots are tried in turn, so a
// rejection is a QUESTION about that root, not the final answer: the caller needs
// to know which stage refused before it decides what to report.
typedef enum {
    SBX_ROOT_OK = 0,
    SBX_ROOT_ESCAPE,     // `..` underflow past "/"
    SBX_ROOT_OUTSIDE,    // normalised, but not under this root
    SBX_ROOT_SYMLINK,    // under this root lexically, escapes via realpath
} sbx_root_verdict_t;

// Runs the three-stage check against one candidate root.
static sbx_root_verdict_t sbx_check_under(const char *path, const char *root, bool *saw_dotdot) {
    char joined[PATH_MAX];
    if (path[0] == '/') {
        snprintf(joined, sizeof(joined), "%s", path);
    } else {
        snprintf(joined, sizeof(joined), "%s/%s", root, path);
    }
    char lexical[PATH_MAX];
    bool dotdot = false;
    if (!sbx_normalize(joined, lexical, sizeof(lexical), &dotdot)) {
        *saw_dotdot = true;
        return SBX_ROOT_ESCAPE; // underflow past "/"
    }
    *saw_dotdot = dotdot;
    if (!sbx_under_root(lexical, root)) {
        return SBX_ROOT_OUTSIDE;
    }
    if (sbx_symlink_escapes(lexical, root)) {
        return SBX_ROOT_SYMLINK;
    }
    return SBX_ROOT_OK;
}

bool pal_sbx_permit(const char *path) {
    t_sbx_reason = PAL_SBX_DENY_NONE;
    t_sbx_detail[0] = '\0';

    if (!path || path[0] == '\0') {
        return sbx_deny(PAL_SBX_DENY_TRAVERSAL, "empty path", NULL);
    }
    if (t_sbx_unsafe > 0) return true; // escape hatch is open

    const char *root = pal_sbx_get_root();
    if (!root || root[0] == '\0') {
        return sbx_deny(PAL_SBX_DENY_NO_ROOT, "no sandbox root configured", NULL);
    }

    // The project root first: it is the common case.
    bool dotdot = false;
    sbx_root_verdict_t verdict = sbx_check_under(path, root, &dotdot);
    if (verdict == SBX_ROOT_OK) return true;

    // The tool's own home, if this program asked for it. Same three stages, so a
    // traversal or a symlink escape out of ~/.owl is rejected exactly as it is
    // out of the project.
    const char *home = pal_sbx_get_tool_home();
    if (home && home[0] != '\0') {
        bool home_dotdot = false;
        sbx_root_verdict_t home_verdict = sbx_check_under(path, home, &home_dotdot);
        if (home_verdict == SBX_ROOT_OK) return true;
        if (home_dotdot) dotdot = true;
        if (home_verdict != SBX_ROOT_OUTSIDE) verdict = home_verdict;
    }

    // Neither root accepted it. Report the most specific thing that was learned
    // about this path rather than a flat "outside": a symlink escape and a `..`
    // are both real attempts to leave, and a caller logging the reason needs to
    // see which one it was. The tool home is consulted last, so its verdict wins
    // only when it is the more informative of the two.
    switch (verdict) {
        case SBX_ROOT_SYMLINK:
            return sbx_deny(PAL_SBX_DENY_SYMLINK, path, NULL);
        case SBX_ROOT_ESCAPE:
        case SBX_ROOT_OUTSIDE:
        default:
            return sbx_deny(dotdot ? PAL_SBX_DENY_ESCAPE : PAL_SBX_DENY_ABSOLUTE, path, NULL);
    }
}

// Convenience for call sites: permit, and on denial record a PAL error so the
// existing error-reporting surface reports it like any other failure.
bool pal_sbx_permit_or_error(const char *path) {
    if (pal_sbx_permit(path)) return true;
    pal_set_error(PAL_ERR_PERMISSION, pal_sbx_reason_text());
    return false;
}

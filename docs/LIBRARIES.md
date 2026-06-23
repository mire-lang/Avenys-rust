# Mire Libraries: `load`, `owl.toml`, and `exports`

This document explains how Mire resolves and loads external libraries, how
the `owl.toml` manifest works, and what happens when you write `load kioto`.

**Important:** `load` imports modules. `use` executes expressions (calls for side effects).
`use kioto::net::http_get` is NOT a valid import — it's parsed as a function call expression
and evaluates the function reference, discarding the result. Imports are done exclusively
via `load`.

---

## Quick Reference

```mire
# Load a library — all modules available with prefixes
load kioto

# Load a specific module
load kioto::net
load kioto::ws

# Call functions with module prefix
set r = net::http_get("https://example.com")
set fd = ws::connect("ws://127.0.0.1:9877/")

# use executes a function for side effects (discards result):
use dasu("hello")
use proc::exit(1)
```

---

## How `load` Works

When Avenys encounters `load kioto`, it follows this resolution path:

### Step 1: Locate the library

Avenys looks for libraries in these locations (in order):

1. **Current project's `owl.toml` dependencies**: If the current project has
   `kioto = { path = "../kioto" }` in `[dependencies]`, Avenys follows that path.

2. **Global modules directory**: `~/.owl/modules/<name>/`

3. **Current directory**: `./<name>/`

### Step 2: Read the library's `owl.toml`

```toml
# kioto/owl.toml
[project]
name = "kioto"
version = "3.11.10"
entry = "mod.mire"

[exports]
strings = "core/strings"
lists   = "core/lists"
dicts   = "core/dicts"
net     = "core/net"
ws      = "ext/ws"
# ...
```

### Step 3: Load the entry point

Avenys opens `mod.mire` (the `entry` file). This is the library's root module.

### Step 4: Process the entry module

`mod.mire` typically contains `load` statements for all sub-modules:

```mire
# kioto/mod.mire
module kioto
load kioto::strings
load kioto::lists
load kioto::net
load kioto::ws
# ...
```

Each `load kioto::<name>` is resolved via the `[exports]` table:
`net = "core/net"` means `kioto::net` → `kioto/core/net/mod.mire`.

---

## The `[exports]` Table

The `[exports]` section in `owl.toml` maps user-facing module names to
filesystem paths (relative to the library root):

```toml
[exports]
strings = "core/strings"    # → core/strings/mod.mire
net     = "core/net"        # → core/net/mod.mire
ws      = "ext/ws"          # → ext/ws/mod.mire
```

Rules:
- Keys are the names users write after `load kioto::<name>`.
- Values are directory paths. Avenys appends `/mod.mire` to find the module file.
- Paths are relative to the library root (where `owl.toml` lives).
- Only modules listed in `[exports]` are accessible. Internal modules without
  an export entry cannot be loaded by external code.

---

## Module Resolution Algorithm

When Avenys processes `load kioto::net`:

```
1. Read kioto/owl.toml
2. Look up "net" in [exports] → "core/net"
3. Construct path: kioto/core/net/mod.mire
4. Parse and type-check mod.mire
5. Register all `pub fn` and `pub struct` declarations in the kioto::net namespace
6. Available as: net::connect, net::http_get, etc.
```

The `module net` declaration at the top of `core/net/mod.mire` defines the
namespace prefix. All `pub fn` in that file become `net::<fn_name>`.

---

## The `[dependencies]` System

Projects declare their dependencies in `owl.toml`:

```toml
# mire-owl/owl.toml
[project]
name = "owl"
version = "0.14.0"
entry = "code/main.mire"

[dependencies]
kioto = { path = "../kioto" }
testlib = { path = "./tests/lib" }
registry = { path = "./code/registry" }
```

Dependency formats:
- **Path dependency**: `kioto = { path = "../kioto" }` — local filesystem path
- **Version dependency** (planned): `kioto = "3.11"` — from the package registry
- **Git dependency** (planned): `kioto = { git = "https://..." }`

---

## `load` — Importing Modules

`load` is the only import mechanism in Mire. It loads modules with their
prefix, so functions are always called with qualified names:

```mire
load kioto::net
load kioto::ws

fn main: () {
    set r = net::http_get("https://example.com")    # qualified
    set fd = ws::connect("ws://127.0.0.1:9877/")    # qualified
}
```

## `use` — Calling for Side Effects

`use` executes an expression statement (calls a function and discards the
return value). It is NOT an import mechanism:

```mire
use dasu("hello")          # print to stdout
use proc::exit(1)           # exit the process
use async::spawn("curl ...") # spawn a background task
```

Any function can be called with `use` — it's just a statement-level call
that discards the return value. Use `load` to import modules, then call
functions with their module prefix.

---

## How `extern fn ... lib "c"` Connects to the PAL

Kioto modules call C functions via `extern fn`:

```mire
# kioto/core/net/mod.mire
extern fn pal_tls_connect: (host :str, port :i64) :i64 lib "c"
extern fn pal_tls_send: (fd :i64, data :str) lib "c"
# ...
```

The `lib "c"` tells Avenys: "this is an external C function, generate an
LLVM `declare` and link against the C runtime."

### The Full Chain

```
kioto/core/net/mod.mire  ← extern fn pal_tls_connect: (...) :i64 lib "c"
     │
     ▼
Avenys → llvm_functions.rs  ← declare i64 @pal_tls_connect(ptr, i64)
     │
     ▼
LLVM IR → clang linker  ← looks for symbol pal_tls_connect
     │
     ▼
pal/linux/pal_tls.c  ← int64_t pal_tls_connect(const char *host, int64_t port)
     │
     ▼
OpenSSL  ← SSL_connect, SSL_read, SSL_write
```

The PAL C files are compiled into `.o` files by clang and linked with the
LLVM-generated object file. The symbol `pal_tls_connect` must match exactly
between the LLVM IR `declare` and the C function name.

---

## Creating Your Own Library

### Minimal example

```
myapp/
  owl.toml
  mod.mire
  code/
    main.mire
```

#### `owl.toml`

```toml
[project]
name = "myapp"
version = "0.1.0"
entry = "mod.mire"

[dependencies]
kioto = { path = "/path/to/kioto" }

[exports]
mylib = "code/mylib"
```

#### `mod.mire`

```mire
module myapp
load kioto
load myapp::mylib
```

#### `code/mylib/mod.mire`

```mire
module mylib

pub fn hello: () {
    use dasu("Hello from mylib!")
}
```

#### `code/main.mire`

```mire
load myapp

fn main: () {
    mylib::hello()
    set r = net::http_get("https://example.com")
    use dasu(r)
}
```

### Important rules for library authors

1. **Every module file must start with `module <name>`**.
2. **Functions you want to expose must be `pub fn`**.
3. **Private helpers are plain `fn`** (no `pub`).
4. **Add every module to `[exports]` in `owl.toml`**.
5. **The entry module (`mod.mire`) must `load` every sub-module**.
6. **Use `load kioto` in `mod.mire` to make stdlib available**.
7. **External C functions use `extern fn ... lib "c"`**.

---

## How the Compiler Finds `owl.toml`

When you compile `code/main.mire`:

1. Avenys walks up from `code/main.mire` looking for `owl.toml`
2. If found, it reads `[project]` (name, version, entry) and `[dependencies]`
3. For each dependency, it resolves the path, reads that project's `owl.toml`,
   and loads its `mod.mire` entry point
4. All `load` statements in all modules are resolved recursively

If no `owl.toml` is found, only the file being compiled is processed.
`load` statements for external libraries won't resolve.

---

## Module Visibility Summary

| Declaration | Visible from | Example |
|------------|--------------|---------|
| `pub fn`   | Any module that loads this module | `pub fn http_get: (...) :str` |
| `fn`       | Only within the same module file | `fn extract_host: (url :str) :str` |
| `pub struct` | Any module that loads this module | `pub struct Point { x: i64, y: i64 }` |
| `extern fn` | Any module that loads this module (if in `pub` scope) | `extern fn pal_tls_connect: (...) :i64 lib "c"` |

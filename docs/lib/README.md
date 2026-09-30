# Libraries

> Library creation, dependencies, and Owl integration.

Version: **4.1.0**
Status: **Stable**
Date: **2026-09-12**

---

## Overview

Mire programs are organized as libraries via `owl.toml`. Owl is the package manager that resolves dependencies, builds projects, and manages the lifecycle of Mire libraries.

---

## Library Types

| Type | Output | Use Case |
|------|--------|----------|
| `bin` | Executable | Applications (default) |
| `static` / `staticlib` | `.a` | Static linking, embedding |
| `shared` / `cdylib` / `dylib` | `.so` / `.dylib` / `.dll` | Dynamic linking, plugins |

---

## Configuration

### In `owl.toml`

```toml
[build]
libt = "cdylib"    # bin | static | shared
crate-type = "cdylib"   # hyphenated alias
```

### Via CLI

```bash
mire build --libt shared
mire build --libt static
```

---

## Library Structure

Based on the design of **kioto** (stdlib) and **mire** (core lib):

```
my-lib/
├── owl.toml
├── code/
│   └── mod.mire          # Library root (entry = "code/mod.mire")
├── core/
│   ├── module_a/
│   │   ├── mod.mire      # Public API + extern declarations
│   │   ├── impl.mire     # Private implementation
│   │   └── types.mire    # Type definitions
│   └── module_b/
│       └── ...
└── tests/
    └── ...
```

### Key Principles

1. **`code/mod.mire` is the library root** - declares `@[no_main]`, loads submodules
2. **Submodules in `core/`** - each is a namespace (e.g., `strings`, `fs`, `math`)
3. **`extern fn` declarations** - at module root or in submodule `mod.mire`
4. **`[exports]` in `owl.toml`** - maps public names to module paths

---

## Library Root (`code/mod.mire`)

```mire
@[no_main]
module mylib

# Load submodules
load mylib::strings
load mylib::fs
load mylib::math

# Re-export at top level (optional)
pub use mylib::strings::{len, upper, lower}
pub use mylib::fs::{read, write}
```

---

## Submodule Pattern (`core/strings/mod.mire`)

```mire
module strings

# C runtime externs
extern fn rt_strings_len: (s :&str) :i64 lib "c"
extern fn rt_strings_to_upper: (s :&str) :str lib "c"

# Public API
pub fn len: (s :&str) :i64 { return rt_strings_len(s) }
pub fn upper: (s :&str) :str { return rt_strings_to_upper(s) }

# Nested namespaces
pub fn starts: () {
    pub fn with: (s :&str, prefix :&str) :bool { ... }
}
```

---

## Exports Configuration (`owl.toml`)

```toml
[exports]
strings = "core/strings"
fs      = "core/fs"
math    = "core/math"
# Consumer does: load mylib::strings
```

Each intermediate directory needs its own `owl.toml` to expose submodules to deeper nesting.

---

## Dependencies (Owl)

Owl manages dependencies through `owl.toml [dependencies]`:

```toml
[dependencies]
kioto = { path = "~/.owl/libs/kioto", version = "2.4.7" }
mire = { path = "~/.owl/libs/mire" }
mylib = { path = "../my-lib" }
```

### Loading Dependencies

```mire
# Package load (from [dependencies]) - direct calls
load kioto
load mire::vec
load mylib::strings

# All calls are direct (no use! needed):
set x = strings::upper("hello")
```

### Local Load (`load!`)

`load!` loads local files without an `owl.toml`:

```mire
load! /utils/string    # Loads ./utils/string.mire
load! math             # Loads ./math/main.mire
```

**Must use `use!` to call**: `set r = use! math::suma(2 3)`

---

## Building

```bash
# Shared library (.so / .dylib / .dll)
owl build --crate-type cdylib

# Static library (.a)
owl build --crate-type staticlib

# Or via mire directly
mire build code/mod.mire --libt shared --release -O3
```

### Output

| Type | Linux | macOS | Windows |
|------|-------|-------|---------|
| `cdylib` | `libmylib.so` | `libmylib.dylib` | `mylib.dll` |
| `staticlib` | `libmylib.a` | `libmylib.a` | `mylib.lib` |

---

## Consuming Libraries

### In Another Mire Project

```toml
# owl.toml
[dependencies]
mylib = { path = "../my-lib" }
```

```mire
load mylib::strings

pub fn main: () {
    set s = strings::upper("hello")
}
```

### From C / Other Languages

```c
void *handle = dlopen("./libmylib.so", RTLD_LAZY);
int64_t (*len_fn)(const char*) = dlsym(handle, "fn_strings_len");
len_fn("hello");
```

---

## Owl as Package Manager

Owl handles all project management:

| Command | Description |
|---------|-------------|
| `owl new myproject` | Create new project |
| `owl build` | Build project |
| `owl run` | Build and run |
| `owl test` | Run tests |
| `owl check` | Analyze only |
| `owl debug` | Debug output |
| `owl load <name>` | Load from registry |
| `owl install <name>` | Install package |
| `owl checkup` | Check and repair |
| `owl deps` | View dependencies |
| `owl lockfile` | Manage lockfile |

---

## Symbol Naming

Mire functions compile to LLVM symbols with mangling:
- `strings::len` -> `@fn_strings_len` (with `env_ptr` first arg)
- Use `nm -D libmylib.so | grep strings_len` to find symbols

---

## Versioning & ABI

- Use **ELF symbol versioning** for shared library compatibility
- Mire ABI v4 symbols use `_M...` mangling scheme
- Breaking changes = major version bump
- Compatible changes = minor version bump (new symbols, optional params)

---

## Related

- [Mire ABI v4](../abi/README.md)
- [Runtime Configuration](../rt/README.md)
- [CLI and Configuration](../compiler/README.md)
- [Avenys Documentation Index](../README.md)

---

## License

GNU General Public License v3.0

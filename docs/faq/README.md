# Frequently Asked Questions

> Common questions about Avenys, Mire, and the toolchain.

Version: **4.1.0**
Status: **Stable**
Date: **2026-09-12**

---

## Getting Started

### How do I install Avenys?

```bash
# Build from source
cd /path/to/avenys
cargo build --release
cp target/release/mire ~/.cargo/bin/mire
```

### How do I create a new project?

```bash
owl new myproject
cd myproject
owl build
owl run
```

### What does `mire` do?

`mire` is the Avenys compiler. It compiles `.mr` and `.mire` source files to LLVM IR, then to native binaries. Commands: `build`, `run`, `test`, `check`, `debug`.

---

## Compilation

### Why does `mire build` fail with a linker error?

Check that all dependencies are in `--lib-dir` or `owl.toml [dependencies]`. Common causes:
- Missing `load` statements in source
- Stale `bin/.cache` - run `rm -rf bin/.cache`
- Incompatible PAL ABI version

### What does "use of undefined value '@foo'" mean?

The compiler found a call to `@foo` that cannot be resolved. This happens when:
- The function is not declared with `extern fn`
- The module is not loaded with `load`
- A bare name is used instead of namespaced form (e.g., `fs::exists` not `fs_exists`)

Use `mire debug --ir` to inspect the generated IR.

### Why does `mire build` produce a very large binary?

Check `[c] runtime` in `owl.toml`. `runtime = "full"` includes all runtime functions. Use `runtime = "minimal"` or `runtime = "none"` to reduce size.

### How do I debug compilation errors?

```bash
mire debug src/main.mire --tokens  # Token stream
mire debug src/main.mire --ast     # AST
mire debug src/main.mire --ir      # LLVM IR
```

### What is the `--no-analysis-cache` flag for?

Disables the incremental compilation cache. Useful when:
- You suspect stale cached analysis
- Testing a clean rebuild
- Debugging compilation issues

---

## Language

### How do I print a value?

Use `dasu(value)` - there is no `println` in Mire.

### How do I define a struct?

```mire
struct Point { x :i64, y :i64 }
set p = Point(x: 1, y: 2)
```

### How do I define a function?

```mire
pub fn add(a :i64, b :i64) :i64 { return a + b }
```

### How do I use external C functions?

```mire
extern fn my_func(arg :i64) :i64 lib "c"
set result = my_func(42)
```

### What are the differences between `i64`, `f64`, `str`, `bool`?

| Type | Size | Description |
|------|------|-------------|
| `i64` | 64-bit | Integer |
| `f64` | 64-bit | Floating point |
| `str` | managed | UTF-8 string |
| `bool` | i64 | true=1, false=0 |

### How do I handle errors?

Mire uses `Result[T, E]` types. Pattern match on the result:
```mire
match result {
    Ok { value } { dasu(value) }
    Err { error } { dasu(error) }
}
```

---

## Runtime

### What is a "runtime tier"?

The runtime tier controls which `rt_*` functions are available and compiled into your binary. Three tiers: `full`, `minimal`, `none`.

### How do I use zero runtime?

```toml
[c]
runtime = "none"
nostartfiles = true
nostdlib = true
libt = "bin"
```

Produces a ~15 KB binary with no external dependencies.

### Why is my program segfaulting?

Common causes:
- Accessing a moved value (use-after-move)
- Out-of-bounds array access
- NULL pointer dereference
- Use-after-free

Use `mire debug --ir` to inspect memory accesses in the IR.

---

## Tooling

### How do I use Owl?

Owl is the package manager for Mire projects:
- `owl new` - create project
- `owl build` - build
- `owl run` - build and run
- `owl test` - run tests
- `owl load` - load from registry
- `owl install` - install package
- `owl checkup` - check and repair
- `owl gc` - garbage collect unused packages

### How do I manage dependencies?

Add to `owl.toml [dependencies]`:
```toml
[dependencies]
kioto = { path = "~/.owl/libs/kioto", version = "2.4.7" }
```

Use `load kioto` to import. For nested modules: `load mire::vec`.

### How do I use the incremental cache?

The cache is automatic - it lives in `bin/.cache`. To disable: `mire build --no-analysis-cache`. To clear: `rm -rf bin/.cache`.

---

## Performance

### How do I optimize for size?

Use `runtime = "none"`, `nostartfiles = true`, `nostdlib = true`, and `-Oz`. Produces ~15 KB binaries.

### How do I optimize for speed?

Use `-O3`, `runtime = "full"`, and `libt = "cdylib"` for shared library linking.

### How do I profile my Mire program?

```bash
mire build src/main.mire --release -O3
perf stat ./bin/release/main
```

---

## Known Limitations

### What is not yet supported?

- WASI Preview 2
- WASI file I/O, networking, processes
- Wasm threads
- Wasm GC/GC pointers
- Multiple memories
- Dynamic linking
- Trait objects / dynamic dispatch
- Multiple inheritance
- Private/protected access modifiers
- Operator overloading
- Async/await (native syntax)

---

## Related

- [Compiler CLI](../compiler/README.md)
- [Runtime Config](../rt/README.md)
- [PAL v4](../pal/README.md)
- [Avenys/Mire Docs Index](../README.md)

---

## License

GNU General Public License v3.0

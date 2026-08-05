# Mire Security Model

## Design Principles

Mire follows three security principles:

1. **Sandbox for macros**: Macros are not AST transformers. They are ordinary
   functions compiled to native code and executed at runtime with the same
   privileges as any other function. A macro cannot see or modify the caller's
   AST, cannot access compiler internals, and cannot execute at compile time.
   There is no interpreter, no const-eval, and no macro-expansion engine.

2. **Capability-based security**: Every operation that can affect the outside
   world (FFI calls, unsafe blocks, system shell access) is gated by an
   explicit allowlist in `owl.toml`. By default, projects opt into strict mode
   where nothing is allowed unless explicitly listed.

3. **Macro hygiene by construction**: Since macros are compiled as ordinary
   functions with lexical scope, there is no token capture, no name collision,
   and no quote/unquote splicing. Variable hygiene follows the same rules as
   any other Mire function.

## Threat Model

### What macros CAN do
- Receive runtime values matching their signature (`:str`, `:i64`, `:bool`, etc.)
- Return values
- Call other Mire functions (including other macros)
- Call `extern fn` functions declared in the program (subject to FFI allowlist)

### What macros CANNOT do
- Access the caller's AST or source code
- Modify the compiler's internal state
- Execute arbitrary code at compile time
- Generate new definitions or splice tokens
- Access the host filesystem, network, or OS directly (except through FFI)

### Real attack surfaces (not macro-specific)
1. **FFI without allowlist**: Any `.mire` file can declare `extern fn system lib "c"` and call arbitrary native code at runtime. The `[security].externs` allowlist gates this in strict mode.
2. **Silent macro injection from dependencies**: A dependency's `[macros]` section can inject code into the consumer's build without a `load` statement. Trust tiers gate this.
3. **Build command injection**: `owl build` executes `[build] compiler` from `owl.toml` as a subprocess. The `[build]` section is gated to bare binary names only.
4. **Cache poisoning**: The incremental cache stores bincode-serialized AST blobs. In strict mode, cache blobs are validated with a checksum.
5. **Path traversal**: `[exports]` and entry paths in manifests can escape the package root. Path containment rejects `..` and absolute paths outside the package root.

## Capability Model

### `[security]` section in `owl.toml`

```toml
[security]
mode = "strict"              # "strict" | "open" (default if section absent: "open")
unsafe = false               # deny unsafe blocks (strict mode)
asm = false                  # deny asm blocks (strict mode)
externs = ["rt_*", "pal_*"]  # allowed extern fn symbol patterns (supports * suffix)
extern_libs = ["c"]          # allowed extern lib names
macros = ["assert", "dbg", "panic", "unreachable"]  # allowed @[macro!] names

[security.deps]
kioto = "ffi"                # trust tier: "code" | "macros" | "ffi"
mire = "macros"
```

### Trust tiers for dependencies

| Tier | Can contribute | Can inject macros | Can declare externs |
|------|---------------|-------------------|---------------------|
| `code` (default) | Functions, types, impls, skills | No | No |
| `macros` | + macro injection via `[macros]` | Yes | No |
| `ffi` | + extern fn/lib declarations | Yes | Yes |

In strict mode, dependencies default to `code` unless explicitly listed in `[security.deps]`.

### Macro sandbox

In strict mode, `@[macro!]` function bodies are further restricted:
- Cannot contain `unsafe` blocks
- Cannot contain `asm` blocks
- Cannot call extern functions that are not in the `[security].externs` allowlist

This ensures that even if a malicious macro file is injected, it cannot escape the sandbox.

## Cache Integrity & Path Containment

### Cache blob checksums

The incremental cache (`bin/.cache`) stores parsed/AST blobs under a filename
that IS the blob's content checksum (`FxHasher`). In strict security mode every
blob read is re-hashed and compared to its filename: a mismatch (on-disk
corruption or deliberate tampering) drops the blob and treats the entry as a
cache miss, so tampered bytes are never deserialized. The corrupt blob file is
deleted so the next `store_blob` rewrites it (stores skip existing files).

Controlled explicitly via `[cache].blob_checksum`; when unset it defaults to
`true` in `mode = "strict"` and `false` otherwise:

```toml
[cache]
blob_checksum = true   # force on (or "false" to force off in strict mode)
```

### Entry & export path containment

`[project].entry` and `[exports]` paths in a manifest must resolve inside the
package root. `check_entry_containment` canonicalizes the joined path and
rejects absolute paths outside the root and relative paths containing `..`
that resolve outside it (`EntryContainment::EscapesRoot`). A non-existent
entry is reported by the normal load path, not silently resolved elsewhere.
This applies to package loading (`resolve_package`) and the CLI's default
entry resolution.

## Migration Guide

### Enabling strict mode

Add a `[security]` section to your `owl.toml`:

```toml
[security]
mode = "strict"
externs = ["rt_*", "pal_*"]
extern_libs = ["c"]
macros = ["assert", "dbg", "panic", "unreachable"]

[security.deps]
kioto = "ffi"
mire = "macros"
```

### What breaks and how to fix it

1. **"Extern function not allowed"**: Add the symbol pattern to `[security].externs`.
2. **"Macro not allowed"**: Add the macro name to `[security].macros`.
3. **"Dependency trust tier insufficient"**: Add the dependency to `[security.deps]` with the appropriate tier.
4. **"Unsafe block not allowed"**: Remove `unsafe` blocks or add `unsafe_allowed = true` (not recommended).

### Upgrading from open to strict

Run `mire check` to identify all violations, then add the necessary entries to `[security]`. The `rt_*` and `pal_*` patterns cover the standard runtime and PAL symbols.

## Hygiene Guarantee

Macros in Mire are hygienic by construction. Since macros are ordinary functions with lexical scope, there is no possibility of variable capture or name collision between a macro body and its call site. This is fundamentally different from macro systems that operate on token streams (e.g., Rust macros, Lisp macros) where hygiene must be explicitly managed.
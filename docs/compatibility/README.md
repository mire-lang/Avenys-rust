# Compatibility

This document describes what this compiler supports and what it requires from
the packages that drive it. It describes the compiler, not any particular
release of its companions.

## Package requirements

Avenys compiles Mire source. The package manager (`owl`) resolves packages and
hands the compiler an instruction; the standard library (`mire`) provides the
core collections and types; utility libraries such as `kioto` are ordinary
Mire packages.

The compiler itself depends on:

- A C11 compiler for runtime and PAL sources.
- LLVM for code generation: `llc`, `opt`, and a target triple.
- A C runtime. The default is glibc; a freestanding build can omit it with
  `runtime = "none"`.

Neither the compiler nor the standard library requires a protobuf runtime. The
build-contract codec is hand-written on both sides, so a build needs no
generated code and no schema compiler. The schema is validated separately in CI.

## Runtime tiers

| Tier | Behaviour |
|---|---|
| `minimal` (default) | Links only the runtime and PAL units the program references. The compiler scans the generated IR, keeps the files it needs, and adds PAL sources only when a PAL symbol is called. |
| `none` | Links no runtime or PAL sources. The program provides its own start-up and runtime symbols. Requires `nostartfiles`/`nostdlib` in most configurations. |

There is no `full` tier. It compiled every runtime and PAL source regardless of
use; measured against `minimal` on the allocation benchmark and the panic
diagnostic smoke, it produced identical output and identical diagnostics at
roughly 74% of the binary size with no measurable runtime difference. A
configuration that still says `runtime = "full"` resolves to `minimal` rather
than failing, so an existing project keeps building.

## Ownership and release

Values whose type owns a heap pointer — `str`, `vec`, `map` — are released when
they leave scope, not only when reassigned. A vector built and returned from a
function is owned by the caller; a vector that goes out of a loop body is
released at the end of that iteration.

A local that borrows storage it does not own is never released. The compiler
decides which destructor to emit from the semantic type during lowering, before
the value reaches the backend, because by then a string, a vector, a map, and a
borrow are all the same machine type.

## Known incompatibilities

- `-L/--link` now expects `name=path` for an ecosystem library. A bare
  directory is still accepted as a native linker directory, so existing
  commands keep working, but it emits `W0061`; the supported spelling is
  `--link-dir <path>`.
- `--bundle` defaults to `static`, which prefers `lib<name>.a` and falls back
  to the shared object with `W0069`. A project that relied on picking up a
  shared object implicitly still links, but now reports the fallback. Use
  `--bundle dynamic` to opt out, or `static-strict` to make the fallback fatal.
- The `bits` namespace is reserved for the bit-level reinterpretation
  intrinsic (`bits::<T>(value)`). A program may not declare a `bits` module in
  the same scope as calls that use the intrinsic; a separate module or package
  resolves the collision.
- The `full` runtime tier is no longer selectable, though the name still
  resolves (see above).
- A collection mutator (`vec::push`, `map::set`) returns the same storage with
  the element added. Assigning its result back to the source is the supported
  form.

## Verification

`cargo test --release` runs the library, CLI, and doc-test suites. The
benchmark suite in `bench/` holds twelve kernels that each exist twice, once
in Mire and once in Rust, performing the same computation. `verify` asserts
that both produce identical output before any measurement is reported, because a
comparison between programs that do the same work measures nothing.

A same-session baseline matters: rerun `verify` and `measure` after a runtime or
ownership change before quoting any number as current.
# Mire Ecosystem Roadmap

**Target version: 4.0.0**

This document records the staged work across the Mire ecosystem (avenys
compiler, kioto runtime, mire standard library, owl package manager, and the
loader). It is the single source of truth for what is being built and in what
order.

## Versioning

- A fixed target version sits at the top of this document: **4.0.0**.
- Five main phases (F0–F5) advance the minor version; the sub-steps inside a
  phase advance the patch version. Each phase closes with a documentation
  sub-phase (+0.1) that lands the minor bump.
- `docs/CHANGELOG.md` groups entries per phase; every commit carries the
  version it produced. Commit messages describe the change and the version,
  with no meta commentary.

| Phase | Scope                                              | Opens   | Closes (+doc) |
|-------|----------------------------------------------------|---------|---------------|
| F0    | Compiler: generics, mutable params, real maps, macro skeleton | 3.18.0  | 3.19.0 |
| F1    | kioto rework (real dicts, honest modules)          | 3.19.0  | 3.20.0 |
| F2    | mire library recreation (`owl new mire`)           | 3.20.0  | 3.21.0 |
| F3    | Macros / `@[derive()]` ecosystem                   | 3.21.0  | 3.22.0 |
| F4    | Reduce built-ins                                   | 3.22.0  | 3.23.0 |
| F5    | Loader dependency injection                        | 3.23.0  | 4.0.0  |

## Macro execution model (decision)

- `@[macro]` makes the parser rewrite the annotated node into a
  `MacroDefinition` (not a `Function`). The symbol table records
  `SymbolKind::Macro`.
- Macro expansion runs as a distinct phase **before** semantic analysis.
- Scope at introduction: **Expression Macros** (`vec!`, `assert!`, `load!`)
  and **Item Macros** (`@[Debug]`, `@[Clone]`, `@[Test]`). **No Statement
  Macros** initially.
- **Security model (enforced, not conventional):** macros run in a restricted
  subset of the Mire interpreter whose context has the `proc::spawn`,
  `fs::*`, and FFI capabilities removed. Because the capability is absent from
  the interpreter, a malicious or buggy macro cannot `rm -rf ~`, read
  arbitrary files, or call into native code. This is a hard boundary, not a
  coding convention.

## F0 — Compiler foundation

- **F0.1 Qualified generic calls.** `helper::push[i64](v 5)` must parse.
  Previously only the unqualified form `push[i64](v 5)` parsed because
  `parse_postfix` only accepted `Expression::Identifier` as a call target with
  type arguments; `Expression::MemberAccess` was rejected. Fixed by routing
  `MemberAccess` through `member_access_name`.
- **F0.2 Mutable parameter syntax.** `:Type mut` is now accepted in parameter
  position and consumed. This only exposes a capability the type checker
  already had (parameters are already treated as mutable internally); it is a
  syntactic affordance, not a new behaviour.
- **F0.3 Real maps and base-type fixes.**
  - Investigate the map accumulation bug tracked in
    `tests/broken_mire/11_maps.mire`.
  - **Vec indexed-assignment corruption** (moved here from the now-removed
    `docs/VEC_INDEXED_ASSIGN_BUG.md`): the native indexed write
    `set v at 4 = 42` on a `vec[i64]` silently loses the value and corrupts
    later reads (observed `read: 1 1 1` instead of `read: 1 42 1`). The
    workaround `lists::set(v 4 42)` is correct because it routes to
    `rt_lists_set_i64`. Fixed-size `arr[T N]` indexed assignment is **not**
    affected. Root cause is in the codegen/ABI of indexed assignment over the
    dynamic vector header (slot not resolved against the correct base pointer,
    or `len`/capacity desynchronized). Fix the codegen of native indexed
    assignment for `vec[T]`.
- **F0.4 Built-ins allowlist surface.** The `[builtins]` allowlist already
  shipped in 3.18 (config `MireBuiltins`, `typeck` enforcement). Verify the
  `module_paths` config field is wired for F5 rather than left unused.
- **+0.1 Documentation phase → 3.19.0.**

## F1 — kioto rework

- Replace the "hollow" modules: `gpu` (currently fake), `async` (simulated
  through `/tmp`), `ed25519` (delegates to openssl) with real implementations
  where feasible, or honest stubs that say what they do.
- Real dictionary type: `map[str i64]` is currently backed by a C runtime
  structure; provide a proper Mire-level map type and surface the operations
  through `mire::map`.
- **+0.1 Documentation phase → 3.20.0.**

## F2 — mire library recreation

- `Arch/libs/mire` is the wrong location: `libs/` is a mirror of the remote
  registry. Recreate the standard library via `owl new mire` at the `Arch/`
  root so it is a first-class local package.
- Structure: `mire::vec::push`, `mire::map::set` (one-word philosophy). No
  `version()` function — `owl.toml` / lockfile already carry the version.
- **+0.1 Documentation phase → 3.21.0.**

## F3 — Macros / `@[derive()]` ecosystem

- Implement the `@[macro]` parser rewrite, `SymbolKind::Macro`, and the
  pre-semantic expansion phase.
- Expression macros: `vec!`, `assert!`, `load!`. Item macros: `@[Debug]`,
  `@[Clone]`, `@[Test]`.
- Run expansion in the restricted interpreter subset (no `proc::spawn`,
  `fs::*`, or FFI).
- **+0.1 Documentation phase → 3.22.0.**

## F4 — Reduce built-ins

- Shrink the built-in allowlist surface; move functionality into libraries so
  the compiler ships fewer privileged primitives.
- **+0.1 Documentation phase → 3.23.0.**

## F5 — Loader dependency injection

- Wire the `module_paths` config field so owl can pass explicit library paths
  to avenys, reducing reliance on the `MIRE_OWL_HOME` environment bridge alone.
- Revisit the modular loader (see Notes) now that the kioto regressions that
  forced its revert are resolved by F1/F3.
- **+0.1 Documentation phase → 4.0.0.**

## Test remake (prepared — pending after macros)

A full rewrite of the integration test suite is deferred until **after F3
(macros)**, so the new tests can use `assert!` / `vec!` and the corrected base
types (`vec[T]` indexed assignment, real maps). This section will be expanded
during F3. Concrete steps to be added then:

- Convert `tests/broken_mire/11_maps.mire` into a passing map test.
- Add a regression test for `set v at idx = val` on `vec[T]` (the F0.3 bug).
- Migrate hand-rolled assertion scaffolding to `assert!`.

## Notes

- **Loader modularization loss.** The modular loader was introduced in commit
  `8340727d` (`src/loader/{mod,renamer,resolver,source,prefix}.rs`) and
  reverted/collapsed in commit `536f0455` ("usa el loader monolitico
  (src/loader.rs)") because it regressed kioto tests (`advanced_literals`,
  `pal_proc_shell_echo`, `E0761`). The `pr-3.18.0` branch does not contain
  those commits. To be revisited in F5 once F1/F3 land. `owl/code` remained
  modular because its nature (per-command, no shared mutable state) differs.
- `Arch/todo.md` tracks a separate owl/PAL migration workstream and is not
  part of this roadmap's commit discipline.

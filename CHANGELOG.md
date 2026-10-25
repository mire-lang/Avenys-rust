# Changelog

## 4.4.13 - 2026-10-25
### Library status() function (phpinfo-style console report)
- **status() added to mire, kioto, sdl, sqlite, wasm libraries.** Each library now
  exposes `pub fn status: () :str` returning a formatted report with name, version,
  description, runtime tier, exports, and status. Usage: `use dasu(mire::status())`
  or `use mire::status()` to inspect what a loaded library provides.
- **owl export / owl -e now verifies status().** The export command checks whether
  the library being exported defines a `status()` function. If missing, it prints a
  warning (does not block export): "Mire Lang organization recommends adding a
  `status()` function to libraries..." with guidance on how to add it.

### MOR registry (mire-lang/libs)
- **kioto: 2.5.0 (stable, branch release/2.5.0) + 3.1.0 (latest, default).** Symlink
  `kioto -> kioto@3.1.0`. The 2.5.0 branch is maintained separately and will lose
  support in the future.
- **mire: 1.2.4 (stable, branch release/1.2.4) + 2.0.0-beta (latest, default).**
  Symlink `mire -> mire@2.0.0-beta`. The 1.2.4 branch is maintained separately.
- **sdl, sqlite, wasm, avenys, owl: latest version only.**
- Registry index at `mire-lang/libs/reg/index.toml` with `default` flags and
  `install_path` for version-suffixed directories.

### CI/CD and installation
- **Install script always gets latest versions.** Uses `get_latest_tag` for owl,
  compiler, and kioto. No hardcoded version pins.
- **Install script uses `owl install` for libraries.** After installing owl +
  compiler, libraries (kioto, mire) are installed via `owl install` using the MOR
  registry, with direct download as fallback.
- **Install script configures MOR registry.** Creates `~/.owl/registries/mor/`
  with `index.toml` defining package versions, branches, and default symlinks.

### Release engineering
- **Compiler version now matches the release tag.** `Cargo.toml`/`Cargo.lock`
  bumped to 4.4.13 so `mire --version` reports the tagged version.
- **Release workflow installs mire/kioto into `~/.owl/libs`.** The owl jobs now
  check out `mire-lang/mire-lib` and pass `--lib-dir`, so owl builds from the
  official libraries without relying on the installer.
- **riscv64 build script is executable and uses the same clean owl config.**
  Removed stale `archive`/`-Wl` entries and the `~/.owl/modules` fallback.
- **Docker toolchain runs install.sh with bash and fails loudly** if the compiler
  asset cannot be downloaded.

## 4.4.12 - 2026-10-24
### CI/CD fixes
- **CI: remove missing archive.c from owl build config.** The generated
  `mire-config.toml` for the `build-owl` job referenced `native/archive.c`, which
  does not exist in the Owl repository. Removed the missing source and the unused
  `archive`/`zstd` libs.
- **CI: install pal runtime directory in build-owl.** The owl build job now copies
  `dist/mire/pal` to `/usr/local/lib/mire/` so the compiler can locate PAL C
  sources at link time.
- **CI: add compiler dependencies (gcc, g++, make, llvm-22, clang-22, lld-22) for
  tcc-based C object compilation.** The `build-owl` job installs a full C toolchain
  so tcc can compile runtime C sources.
- **CI: remove `-Wl,--disable-new-dtags`/`-Wl,-rpath` cflags from owl
  mire-config.toml.** These linker flags were being passed to `tcc -c` for C object
  compilation, causing failures.
- **Fix tcc error parsing for crypto library auto-detection.** Mire's linker retry
  logic only recognized ld-style `undefined symbol` in tcc's stderr. tcc 0.9.28rc
  emits `unresolved reference to` instead. Both forms are now recognized.
- **install.sh: default repositories changed to `mire-lang/` org.** `REPO_OWL`,
  `REPO_COMPILER`, and `REPO_KIOTO` now default to `mire-lang/owl`,
  `mire-lang/Avenys-rust`, and `mire-lang/Kioto` respectively. `Evelynx-Dev/*`
  references removed.

## 4.4.11 - 2026-10-24
### CI/CD and installation
- **CI: remove missing archive.c from owl build config.** The generated `mire-config.toml` for the `build-owl` job referenced `native/archive.c`, which does not exist in the Owl repository. Removed the missing source and the unused `archive`/`zstd` libs.
- **CI: install pal runtime directory in build-owl.** The owl build job now copies `dist/mire/pal` to `/usr/local/lib/mire/` so the compiler can locate PAL C sources (`pal_bridge/helpers.c`, `pal/linux/pal_linux.c`, etc.) at link time.
- **CI: add compiler dependencies (gcc, g++, make, llvm-22, clang-22, lld-22) for tcc-based C object compilation.** The `build-owl` job installs a full C toolchain so tcc can compile runtime C sources.
- **CI: remove `-Wl,--disable-new-dtags`/`-Wl,-rpath` cflags from owl mire-config.toml.** These linker flags were being passed to `tcc -c` for C object compilation, causing failures. Linker flags belong in the link step, not the compile step.
- **Fix tcc error parsing for crypto library auto-detection.** Mire's linker retry logic only recognized ld-style `undefined symbol` in tcc's stderr. tcc 0.9.28rc emits `unresolved reference to` instead, so `-lsodium` was never auto-added. Both forms are now recognized.
- **install.sh: default repositories changed to `mire-lang/` org.** `REPO_OWL`, `REPO_COMPILER`, and `REPO_KIOTO` now default to `mire-lang/owl`, `mire-lang/Avenys-rust`, and `mire-lang/Kioto` respectively. `Evelynx-Dev/*` references removed.

## 4.4.10 - 2026-10-20
### CI/CD and migration
- **CI: migrate to official `mire-lang` repos.** Replaced `Evelynx-Dev/Kioto-1@pr/2.5.0` with `mire-lang/Kioto@master` and `mire-lang/owl@pr/1.2.0` with `mire-lang/owl@main` in `.github/workflows/ci.yml` and `release.yml`. All `Evelynx-Dev` references removed from workflows.
- **Compiler fixes pushed to `main`:** absolute-path native `.so`/`.a` linking for tcc, Pointer↔collection unification, Pointer↔`Ref`/`RefMut`/`Str` coercion, `pal.*` stubs injected into typecheck/builtins, enum payload-less variant equality, auto-inject `load mire::aux::pal` in test runner, suffix-based call resolution for flattened module prefixes, literal-index parse guard, skip helper modules with no `main`/`@[test]`, relaxed proc arg parsing.
- **FFI sandbox tests:** replaced hardcoded `/home/Evelyn/.owl` paths with `/tmp`-based paths for CI compatibility.
- **Local verification:** full test suite passes 324/324 against `/tmp/official/libs` (mire-lib + Kioto from `mire-lang`).

### Owl v1.3.0
- Fixed secondary CLI commands (`--version`, `info --json`, `load`, `new`) that suffered double-free from Mire runtime MSS string handling. Rewrote `compiler_cmd()`, `compiler_version()`, `info::run()`, and `new::run()` without problematic `str mut` reassignments. Added `util::copy_args()` helper.
- Build (`-d`, `-r`, `-O`, `--release`, `--debug`) and `run` work correctly.


All notable changes to Avenys will be documented in this file.

## 4.3.13 - 2026-10-09

### Fixed
- **Owned values are released when they go out of scope.** They were released
  in exactly one place — the rebind in `set x = ...` — so a `return`, the end of
  a loop body, and falling off the end of a function all released nothing. A
  function that built a vector and returned a number leaked the vector on every
  call. Measured with `pal_mem_process`, 2,000,000 iterations building one
  `vec[i64]` each grew the process from 6.3 MB to 198 MB; it now holds at
  6.4 MB. Releasing at scope exit skips locals that escape — a returned vector
  is left to the caller — never releases `Ownership::Borrowed`, and stops at
  `break`, `continue`, and `return`, where the remaining statements are not on
  that path. **This does not speed up the C2 allocation kernel** (53.35x,
  1387 ms, against a previous 55.70x): the leak was a correctness defect, while
  C2's cost is malloc/free churn eight million times over, which is what the
  slab allocator is for.
- **`bits` is no longer reserved against the program that defines it.** The
  bitcast intrinsic's `bitcast_target` claimed every `bits::<identifier>` call,
  so any package exporting a `bits` module failed to build with
  `unknown bitcast target type in `bits::<bits.double>(value)`` — a bitcast
  diagnostic for a call unrelated to bitcasts, with the function name doubled.
  The intrinsic now claims a suffix only when it names a real type, and other
  calls reach ordinary resolution; a genuine typo now gets the accurate
  "unknown function" diagnostic. Four unit tests cover this, and the regression
  test was confirmed to fail with the fix reverted.

### Added
- **Mire-hosted runtime bootstrap modules** (Phase B5): `core/bits/mod.mire`,
  `core/format/mod.mire`, and `collections/slice/mod.mire`, each carrying a
  `BOOTSTRAP PROOF` header. They hold bit counting, decimal conversion and
  padding, and read-only vector helpers. They resolve no package — the runtime
  cannot require Kioto, and depending on `mire::str` would make the runtime's
  text helpers depend on the package they help implement — so `format.mr`
  declares `rt_strings_substr` directly and otherwise uses only the `len`
  builtin. The C runtime remains the implementation of record.

### Changed
- **`full` runtime tier removed.** The tier compiled every runtime and PAL source
  regardless of use. Measured against `minimal` on the C2 allocation kernel and
  the panic diagnostic smoke, it produced identical output and identical
  diagnostics at roughly 74% of the binary size (203,256 B → 150,064 B for C2)
  with no measurable runtime difference across three timing replicas. The tier
  and its two selection branches are removed, and `filter_pal_decls` no longer
  takes a tier because PAL declarations are filtered by use rather than by tier.
  **Compatibility:** `"full"` still parses, in both `RuntimeTier::parse` and the
  serde deserializer, and resolves to `minimal`, so an existing project keeps
  building and keeps producing the same diagnostic.
  `scripts/test-runtime-io.sh` keeps that case covered deliberately.
  Documentation updated in `README.md`, `docs/cli`, `docs/compiler`, `docs/faq`,
  and `docs/rt`.
- **Managed release contract pinned and the last silent release removed:** new
  `tests/runtime_managed_release_smoke.c` (driven by
  `scripts/test-runtime-managed-release.sh`) verifies that a managed string
  releases by refcount, that the last release returns its block to a size-class
  free list and a same-size allocation reuses it, that a foreign pointer —
  stack or heap — is never read as a managed header, and that releasing `NULL`
  or an already-released pointer does not corrupt the arena. The
  `MirOp::Add` string-concatenation path no longer decides to release its left
  operand by matching the LLVM type name against `"ptr"`; that inference could
  not distinguish a string from a vector, a map, or a borrow, and it duplicated
  the decision A1 moved into `MirOp::Drop`'s `Ownership`. Its condition now
  matches `MirOp::Concat`'s lowering. No behavior change.
- **Runtime host boundary closed:** runtime C sources now reach the host only
  through `pal_*` capabilities. `grep` for host headers (`unistd`, `sys/*`,
  `poll`, `errno`, `fcntl`, `signal`, `time`, `process`) and for direct host calls
  (`fork`, `execvp`, `waitpid`, `pipe`, `dup2`, `mmap`, `munmap`, `poll`,
  `isatty`, `read`, `write`, `open`, `close`, `kill`, `_exit`) both return zero
  matches in `src/runtime`. The last such dependency was `input.c`, which read
  `errno` to retry `EINTR`; the retry now lives in `linux_stdin_read()`, inside
  the capability that issues `read(2)`. The runtime consumes 53 distinct
  `pal_*` capabilities whose implementations live in `runtime/pal_bridge/`.
  Remaining libc use in runtime C is `snprintf` (formatting only, no I/O) and
  the `malloc`/`realloc`/`free` family routed through `pal_alloc`,
  `pal_realloc`, and `pal_free`. This is a boundary invariant, not a claim of
  cross-platform parity: Windows remains unimplemented in the PAL.
- **Runtime source layout:** C runtime modules are organized by responsibility
  under `core/`, `memory/`, `collections/`, `std/input/`, `std/output/`,
  `std/error/`, and `pal_bridge/`.
  Compiler source discovery and release packaging now recurse through those
  directories. This is a source-layout change only; runtime behavior and host
  dependencies are unchanged.
- CI uses two Cargo test jobs to avoid memory pressure during concurrent LLVM
  links, and release packaging verifies that nested runtime modules are
  included in compiler archives.
- **Minimal-runtime input calls:** `ireru` is now retained by runtime symbol
  analysis, receives its LLVM declaration, and selects the new input module.
  Previously it was accepted by validation but emitted as an undeclared LLVM
  call. A piped-input runtime smoke test now protects this path in CI.
- **Monotonic runtime timing uses the PAL:** elapsed-time formatting now calls
  `pal_time_monotonic_ns()` through an isolated `core/time.c` module. Linux and
  WASI provide the monotonic clock, and minimal builds include the PAL backend
  only when this runtime function is used.
- **Standard input uses the PAL:** runtime reads, readiness checks, and
  terminal detection no longer invoke POSIX input calls directly. Linux
  implements all three operations; WASI supports reads and explicitly reports
  readiness and terminal detection as unsupported. CI compiles the affected
  modules with pinned WASI SDK 27.0, which remains a development-only tool.
- The runtime `std/output` and `std/error` APIs now use PAL byte writes and
  PAL-managed error flags. Runtime prompts and numeric/string output no longer
  call host stdio output functions directly.
- Panic and safety diagnostics now share PAL-backed formatting and stderr
  writes. Minimal-runtime source selection includes the diagnostic PAL module
  transitively with safety and managed-memory support. CI preserves and checks
  the historical panic exit statuses in minimal and full runtime builds.
- Runtime panic and managed-allocation failure paths now terminate through
  `pal_process_exit()` rather than calling the host C `exit()` function
  directly. Minimal and full panic smoke tests preserve their existing status
  codes, and the WASI backend compiles the same PAL operation. No installer or
  CI package dependency changed.
- The full managed-string arena now reserves and releases its fixed region via
  PAL virtual-memory operations; POSIX mapping calls and headers no longer leak
  into the runtime allocator module. Linux retains lazy `MAP_NORESERVE`
  semantics; WASI reports this capability as unsupported.
- Runtime whole-file reads, file hashing, hex-to-file writes, and web-log
  appends now use sandboxed PAL root/file handles instead of `stdio` file
  operations. The Linux backend also passes a creation mode to `openat2` only
  when `PAL_OPEN_CREATE` is set; non-creating reads previously failed because
  Linux requires that mode argument to be zero. A binary-file and sandbox
  regression smoke test now runs in CI using the existing C compiler and
  libsodium CI dependencies; no installer packages were added.
- The runtime's controlling-terminal prompt now uses `pal_tty_read_line()`.
  Linux reads from `/dev/tty`; WASI declares the capability unsupported, and
  the runtime preserves its existing non-interactive `"y"` fallback. A PTY
  smoke test exercises both interactive input and the detached-process
  fallback in CI without adding a package dependency.
- Runtime argv-based process capture now uses PAL process creation and channel
  reads instead of direct process, pipe, and wait calls in runtime code. Linux
  supports sandbox-authorized working directories, stdout capture, optional
  stderr merging, and conventional signal exit statuses. A focused smoke test
  covers these behaviors; this is a Linux process-helper migration slice, not
  full cross-platform process support. No installer dependency changed.
- Vector storage allocation and release now use the PAL allocator API, with
  checked conversion to the PAL's signed allocation-size type. Minimal runtime
  source selection includes the PAL backend when vectors are used, and the
  WASI compile smoke covers the vector module. A same-session C2 comparison
  stayed within the benchmark harness's noise threshold (1,361 ms before,
  1,381 ms after; Rust 26 ms in both runs), so this is a portability-boundary
  change, not a measured performance gain. No installer dependency changed.
- Temporary buffers in runtime input line, byte-count, and read-all helpers now
  use PAL memory operations. Capacity growth checks PAL's signed size limit and
  prevents overflow; the input smoke now tests a line longer than the initial
  buffer. This is a scoped allocation-boundary migration, not a complete runtime
  allocator conversion. No installer dependency changed.
- The math median helper now allocates and releases its temporary sorted copy
  through PAL memory functions, with a checked size conversion. Minimal-runtime
  source selection and the WASI compile smoke include the math module; a C test
  verifies odd/even medians and matching PAL allocation/release counts. No
  installer dependency changed.
- `Maybe` and `Result` heap descriptors now use PAL allocation and release
  functions. Their representation, payload ownership, and destructor behavior
  are unchanged. Native smoke coverage verifies Some/Ok/Err values and balanced
  allocation/release calls; the WASI compile check includes this module. No
  installer dependency changed.
- The minimal managed-string allocator now uses PAL allocation functions for
  raw copies, formatted buffers, owned strings, registry growth, and release.
  Failed registry growth now frees the unregistered allocation and reports
  failure; size and capacity arithmetic is checked. Ownership tracking and the
  rule that non-owned/static values are not freed remain unchanged. A focused C
  smoke checks allocation counts, oversized requests, and literal preservation.
  No installer dependency changed.
- PAL bridge base64/PEM/Ed25519 temporary buffers, PAL-owned process output, and
  the per-thread shell result buffer now use PAL allocation/reallocation/release
  functions with checked size conversions. File smoke coverage verifies
  base64 output and malformed-input rejection; process capture coverage remains
  green. No installer dependency changed.
- Dictionary container storage now uses PAL allocation, growth, and release;
  stored key/value ownership semantics are unchanged. A growth regression test
  found and fixed inconsistent signed/unsigned hash modulo during bucket
  rehashing. CI and the WASI source compile gate cover the map modules. This is
  a portability-boundary change, not a measured performance claim.
- Full-runtime raw string helpers, argument-vector buffers, string-padding
  scratch buffers, and map-formatting temporaries now use matching PAL
  allocation/release calls. Minimal-runtime selection marks the affected
  string module PAL-dependent; the WASI compile gate covers it. Focused C
  allocator tests and Mire padding tests protect the pairing and output. No
  installer dependency changed.
- CPU elapsed-time formatting now reads `pal_cpu_time_ms()` instead of calling
  the C library `clock()` directly. Its argument is documented as a timestamp
  from that PAL clock, and a focused smoke test protects the millisecond-unit
  subtraction. The runtime string module no longer includes POSIX headers for
  this path.
- The legacy minimal-string source now uses PAL allocation/reallocation/release
  for its raw string operations and argv cleanup. A focused C smoke verifies
  output and balanced allocations, and the WASI compile gate includes the
  source. This file is not currently selected by the runtime source graph, so
  this is portability cleanup rather than a runtime behavior change.

## 4.3.12 - 2026-10-09

### Fixed
- **Collection drops now use the semantic destructor.** `MirOp::Drop` carries
  ownership from lowering, so vectors and maps use `rt_list_free` and
  `rt_dict_free` instead of the managed-string free path. This fixes the
  collection leak caused by ordinary malloc-backed values being ignored by
  `rt_managed_free`. The related `vec.push` assignment-reuse ownership defect
  was fixed before accepting any post-fix performance number; the preliminary
  3.12x result is invalidated in the benchmark documentation.
- **Binary expression type ascriptions survive type checking.** The trailing
  ascription in `1 + floating :i64` belongs to the whole expression. Integer
  XOR also infers its result from its operands rather than a boolean parser
  default.
- **Registered but unsupported builtins fail before code generation.** The
  builtin registry now has an exhaustive explicit lowering disposition, and
  unsupported calls produce a direct type-checking error. User-defined
  functions retain precedence over builtin fallback. Qualified unresolved
  calls are no longer skipped by codegen validation.
- **Type documentation uses accepted syntax.** Invalid declarations such as
  `set value :i64 = expression` have been replaced with
  `set value = expression :i64`.
- **BETA compatibility regressions corrected:** comparisons involving `&str`,
  the accidental `/tmp/owl.toml` configuration path, and unitary `main`
  functions. These frontend compatibility cases are recorded explicitly so
  ecosystem projects can include them in their regression audit.

## 4.3.11 - 2026-10-08

### Fixed
- **`env::get` was a phantom builtin.** It was registered with a return type, so
  type checking accepted it, and no phase ever lowered it: there is no `getenv`
  in the runtime and no `env` module in the standard library. The build reached
  llc and failed on a call to an undefined `@env.get`, naming neither the
  missing builtin nor the reason it was missing.
- Every other `env_*` spelling is correctly rejected, so `env::get` was the only
  name where an unknown call produced invalid IR instead of a diagnostic.
  Registering a name is a promise that code generation keeps; the phantom entry
  is withdrawn rather than implemented, since exposing environment variables is
  a feature and needs a runtime function behind it.

## 4.3.10 - 2026-10-08

### Fixed
- **`-Os` and `-Oz` failed on every build.** Both are accepted on the command
  line and listed in `--help`, and both were passed straight through to `opt`
  and `llc`. LLVM removed those two pipelines, so `opt` rejected them with a raw
  LLVM error and `llc` rejected them again with "invalid optimization level".
- The size preference is a **function attribute**, not a pipeline name: LLVM
  wants O2 plus `optsize`/`minsize` on each function. The attribute is now
  applied to every definition, and both tools get their level from one place --
  they had drifted into two separate copies of the same match, which is how the
  fix needed applying twice.
- Found while benchmarking: sweeping the optimization levels is how it turned
  up. Note that `-Oz` is genuinely slower than `-O2` on compute-heavy loops,
  because `minsize` suppresses vectorization -- that is the attribute working,
  not a regression.

## 4.3.9 - 2026-10-08

### Fixed
- **Two functions in one module, each defining a closure, could not both
  compile.** The closure counter restarted at zero for every function, so both
  produced `closure_0` and `closure_env_0`. Which way that failed depended on
  the captured shapes: llc rejected the module with "invalid redefinition of
  function", or the second definition quietly replaced the first's environment
  struct and the surviving one had no fields, so the capture GEP was emitted
  against `{ }` and llc reported "invalid getelementptr indices".
- **Neither shape failed on its own.** A capturing closure, a non-capturing one
  and a nested one each compiled fine in isolation, as did every pair of them
  in a single function. It took two functions in the same module, and the order
  they were declared in decided which of the two errors came out. The counter
  is now one per module.

## 4.3.8 - 2026-10-07

### Fixed
- **Every raw pointer was single-use.** Passing a pointer to one function made it
  moved, so reading it once and then handing it to anything else was rejected as
  a use after move. The error landed on the second call and never mentioned
  pointers.
- **`result::is::ok(r)` followed by `result::unwrap::i64(r)` was a use after
  move** -- the pattern the Result tests themselves use, split across two
  functions so neither one tripped it. `is::ok` inspects a Result without
  consuming it, and rejecting the pair made the module close to unusable.
- Cause: `ptr` is not a recognised type name, so `(ptr :ptr)` parameters arrived
  at the borrow checker as an unresolved type, and an unresolved parameter type
  was being read as owning. A type the compiler does not know is not evidence
  of ownership, and since ownership is static here and there are no
  destructors, guessing "owned" only ever rejects correct programs.
- `DataType::Pointer` joins the copy types, which it already should have been:
  a raw pointer is a non-owning address, and ownership is tracked through
  `Ref`/`RefMut` and the borrow checker rather than through pointers.

## 4.3.7 - 2026-10-07

### Fixed
- **`find` compiled to nothing.** The loop body was never lowered, so
  `find x in [1 2 3] { dasu(x) }` built, linked, ran and printed nothing -- no
  error at any stage, and an exit status of zero. It was the worst of the
  silent bugs: the program was accepted, linked and run, and simply did the
  wrong thing. `Statement::Find` has arms in the parser, in semantic, in
  monomorphize and in both incremental passes, but none in the MIR lowering, so
  it fell through to a catch-all.
- **`find` and `for` are now one arm.** They differ only in `for`'s optional
  index binding, so the loop is lowered once rather than twice.

## 4.3.6 - 2026-10-07

### Fixed
- **`move::(src) to dst` did not compile.** The statement reached the MIR
  lowering's catch-all arm, so no slot was ever created for `dst`. Type checking
  and the borrow checker had both already accepted the binding, so the program
  reached llc and failed there with `use of undefined value '@dst'` -- an error
  pointing at the *use* of the moved value, on a line after a move that was
  itself completely fine. It now lowers like `set dst = src`.
- **A move onto an already-bound name kept the old value.** `dst` was never
  written, so moving over it left `dst` reading whatever it had before. Silent,
  and wrong in the direction that loses data rather than crashing.

## 4.3.5 - 2026-10-07

### Fixed
- **A type ascription followed by a binary operator was a syntax error.** `10 :i64
  / z` did not parse: the ascription was only recognised at the outermost level,
  after every binary operator had been consumed, so the parse stopped at the `:`
  and the `/` was left dangling. With `*` it was worse than a syntax error --
  `*` there is a dereference, so `10 :i64 * z` failed with "cannot dereference
  non-reference type I64", pointing at the literal rather than at the missing
  parentheses. The colon is now handled in the postfix chain.
- **Which expression an ascription takes now depends on what follows it.** Both
  readings are wanted, so precedence alone cannot decide: `10 :i64 / z` ascribes
  the operand, while `i == n :bool` has to ascribe the comparison, because
  ascribing `n` would compare an i64 with a bool. The colon is parsed
  speculatively and the decision made afterwards -- an operator following means
  the operand took it, otherwise the tokens are put back and the enclosing level
  ascribes the whole expression, which is what it always did.
- **A `:T mut` or `:T const` binding suffix is no longer read as an ascription.**
  `set at_end = i == n :bool mut` declares a mutable bool; taken as an ascription
  of the operand it became a comparison against a bool and the `mut` was left
  stranded. This is what the speculative parse above keeps working.
- **A comma-separated list literal was a syntax error.** `[1, 2, 3]` parsed one
  element, stopped on the comma, and then tried to read the comma as the next
  element; `[1]` worked, which is why it went unnoticed. Both separators are now
  accepted, so `[1 2 3]` and `[1, 2, 3]` are the same list.
- **A bracketed `key: value` literal could not be selected at all.** The test
  that chose between list and map looked for a comma at bracket depth one, but
  the caller had already consumed the opening bracket, so the depth never reached
  one and the branch was unreachable -- `[a: 1]` failed with "expected type"
  because the key's colon was read as an ascription. The discriminator is the
  top-level colon, which is what actually differs between the two forms.
- **The colon in `{a: 1}` is punctuation, not an ascription.** Once the postfix
  handles colons, a key-value literal needs the key shielded from it. The guard
  covers the key only, since an ascription on the value is still an ascription.

## 4.3.4 - 2026-10-07

### Fixed
- **A bare `return` in a function that declares a value was accepted, then
  emitted invalid IR.** `unify_types` treats `DataType::None` as "no constraint,
  adopt the other side", which is what statement inference wants and what a
  return must not get, so `return` with no operand passed the type checker in a
  `:u8` function. The function was then emitted with a `void` terminator against
  an `i8` signature, and the only complaint came out of llc with no source
  location and nothing pointing at the return that caused it. A bare return in a
  void function is unaffected, so early exits keep working; both shapes are
  pinned in `src/compiler/typeck_tests.rs`.
- **An integer widened on the way out was emitted at its original width.** The
  checker accepts `return small()` from a `:i64` function when `small` returns
  `:u8`, and every other implicit conversion is materialised during lowering —
  `set x = small() :i64` worked. Returns were the one that was not, so the value
  reached the backend at its own width and llc rejected `ret i8` out of an `i64`
  function with no source location. The conversion now happens in the lowering,
  through the same `emit_convert` a `let` ascription uses, which is what keeps it
  correct for unsigned values: the backend sees only LLVM type names, which carry
  no signedness, so a widening applied there would sign-extend and turn 200u8
  into -56. `tests/Functions/returns.mire` pins both the signed and the unsigned
  direction.

### Removed
- **A leftover debug `eprintln!` in `check_program_types`.** It printed the
  statement count on stderr for every compilation.

### Fixed (build hygiene)
- **`pal_proc_shell` was declared `const char *` but documented and used as
  `[PAL-OWNED]` malloc'd memory.** The caller has to `free()` it, so assigning it
  to a `char *` discarded the qualifier and clang warned on every build. The
  declaration now matches what the function actually returns.

## 4.3.3 - 2026-10-06

### Added
- **Path sandbox in the PAL** — every absolute-path entry point now consults
  `pal_sbx_permit` before reaching the kernel. This is a security control rather
  than a lint, and that distinction is the whole reason it sits in the PAL: a
  policy enforced by the language layer is advisory, because a program can
  declare its own extern and call the kernel directly. `pal_root_open` was
  ungated before, which handed out a capability for the whole filesystem to
  anyone who asked for `/` and made the relative API's confinement meaningless.
  The default root is the working directory, captured lazily. Three stages run
  against it: lexical normalisation of `.` and `..`, a prefix test that requires
  the separator, and a `realpath(3)` re-check of the deepest *existing* ancestor
  so a symlink planted inside the root cannot point at `/etc`. The last stage
  needs the deepest-existing-ancestor trick because `realpath(3)` fails outright
  on a path that does not exist yet, and creating a file through a symlinked
  parent is exactly the case worth catching.
- **`pal_sbx_set_tool_home()` — a second authorised root.** A tool that keeps a
  store which must outlive any one project needs a home outside the project
  tree: a project tree cannot be shared between checkouts, and copying every
  dependency per project defeats the point of having a registry. The home is
  authorised *separately* rather than folded into the main root, because widening
  the root would give every program in every project write access to the whole
  home directory. Exactly one directory is granted, to the one program that asks
  for it, and it is never inferred from the environment — a project must not be
  able to widen its own sandbox by exporting a variable. The grant authorises no
  execution either: a file dropped into the store is inert data, and the
  installer only activates a package whose contents match the registry hash.
- **`pal_proc_shell()`, runtime-gated.** A shell takes a string, so every
  metacharacter in it is a command-injection vector. The symbol is compiled in
  but denied unless the calling thread holds an open unsafe scope, so a program
  can be deployed once and decide per call site. The default answer is no.



### Fixed
- **`rt_sbx_*` did not link on the minimal runtime tier.** `rt_sbx_get_root` and
  friends are declared in `helpers.c`, but the tier only extends the C source
  list with `pal/core` when the IR mentions a `pal_*` symbol. A program declaring
  an `rt_sbx_` extern on its own therefore failed to link with an undefined
  reference even though the symbol was sitting in a file it already had. The
  symbol-to-file map now has an explicit `rt_sbx_` arm.

### Changed
- **Removed `check_command` and `lsp_command`.** Neither was reachable from the
  dispatch and neither had a test, so both sat behind `#[allow(dead_code)]`,
  which is a way of asking the compiler to stop reporting a question rather than
  an answer. `lsp_command` in particular was 170 lines parsing a `--lsp` flag
  that no command-line entry point accepted. Deleted rather than left parked;
  the LSP surface has to arrive with its dispatch and its tests.
- **`integer_bit_width` is `#[cfg(test)]`.** It was reachable only from
  `src/types/tests/`, and the module-level `#[allow(dead_code)]` would also have
  silenced a real future caller that forgot to wire it up.
- **`is_float_type`, `llvm_version` and the unused `#[allow(dead_code)]` on
  `collect_c_files` are gone.** The first two had no callers at all; the third
  was suppressing a warning for a function that is used, recursively, on its
  second line.
- **`src/cli/test.rs`: the test module moved to the end of the file.** Clippy's
  `items_after_test_module` was correct — the module was followed by five
  functions that belong with it.
---

## 4.3.2 - 2026-09-29
A patch release fixing a data race in the incremental cache that only shows up
when several compiler threads initialise a cold cache at once, which is exactly
what `mire test -j N` does.

### Fixed
- **A module path could no longer be named after a keyword** — this is a
  regression introduced by the `in`-as-a-name change below, and it is recorded
  here because the `in` fix is what surfaced it. `expect_ident` had always
  allowed `new`, `drop`, `move`, `own`, `set`, `to` and `is` as names. Replacing
  it with the new `expect_path_segment` in `parse_module_statement` and in the
  `load` path narrowed it to `in`/`is`/`to`/`of` and silently took the others
  away. The first casualty was `module new`, the name of owl's scaffolding
  module, which failed the `build-owl` CI job with
  `error[E0003] Expected a module path segment` before owl's `main` was ever
  reached. No local test caught it, because nothing in the test corpus is named
  after a keyword. A path segment is now any token whose surface is a word, the
  same test `expect_member_name` uses, which makes it a strict superset of
  `expect_ident` by construction; punctuation is still rejected, so
  `load a::::b` remains a syntax error. `module_name_accepts_every_keyword_a_path_segment_does`
  and `a_path_segment_is_never_narrower_than_an_identifier` in
  `src/parser/mod.rs` walk the whole set and both call sites, and both were
  checked to fail against the enumerating version.
- **`dasu` and `ireru` never actually flushed** — the flush was emitted *before*
  the write it was meant to push out. `compile_inst` assembles an instruction by
  emitting its `extra` lines first and the returned line last, which is right for
  setup (allocas, coercions) but wrong for a trailing flush: the generated code
  called `fflush` and then `printf`. The symptom was invisible on a terminal,
  because line buffering hid it, and appeared as soon as stdout was a pipe or a
  file. A program alternating `dasu` with a write to stderr came out reordered —
  `two 1 four 3` instead of `1 two 3 four` — because stdout stayed in its buffer
  while unbuffered stderr went straight out, and nothing flushed stdout until
  process exit. `ireru` had the same defect, meaning an input prompt could reach
  the terminal late. Both now emit write-then-flush. The ordering is pinned by
  `dasu_flushes_after_printing`, `ireru_flushes_after_writing_the_prompt` and
  `dasu_still_emits_exactly_one_flush_per_call` in
  `src/compiler/mir/codegen/mod.rs`, which build a real `MirProgram` and check
  the emitted IR.
- **Parallel `mire test` could fail with `Cannot write cache file ... No such
  file or directory`** — this is what took CI down. The failing job reported
  `Passed: 268 - Failed: 8`, and the one failure it managed to print was
  `tests/Builtins/scalars.mire` dying with

      error[E0015] Runtime Error
      Cannot write cache file '/tmp/avenys-tests-cache/blobs/ec96b757843f9848':
      No such file or directory (os error 2)

  Two defects combined to produce it. `load_with_settings` created the
  `index/`, `blobs/` and `wal/` directories *before* it took the
  initialisation lock, so the thread that won the lock could `remove_dir_all` a
  directory another thread had just created, and that thread then failed a
  write against a path it had already made. Compounding it, the lock lived
  *inside* the cache directory, so the wipe deleted the very lock that existed
  to prevent it: the holder carried on believing it was exclusive while another
  thread created the lock directory again and began a second wipe.
  The directory structure is now built under the lock and after any wipe, and
  `wipe_cache_dir` removes the cache's children one by one while explicitly
  skipping the lock instead of `remove_dir_all`-ing the directory it lives in.
  The lock stays inside `bin/.cache`, which projects already ignore, so no lock
  bookkeeping leaks into the project tree.
- **A waiter could return before the cache was actually initialised** — the old
  waiter polled until the lock directory disappeared and then returned, which is
  not the same thing as "the cache is ready". A holder that died between its
  wipe and its version write left the lock gone and the cache unversioned, so
  the waiter walked into a cache that the next arriving thread would then wipe
  underneath it. The exit condition is now the version file itself, and a
  waiter that finds the lock free — or stale, because a holder crashed — takes
  the lock and performs the initialisation itself rather than assuming someone
  else did.
- **The waiter's own last-resort path re-entered the race it existed to
  prevent** — on hitting its give-up deadline it removed the lock and called
  `init_cache` directly, i.e. it wiped the cache while *not* holding the lock.
  That is the same defect as above: removing someone else's lock does not make
  the cache yours, and an unsynchronised wipe is how the CI failure started. It
  only ever checked that the outcome was correct, never that the lock was held,
  so it satisfied every test while reintroducing the race. The path now breaks
  the lock and loops to re-acquire it through the same atomic `create_dir` that
  grants the lock in the first place, re-arming the deadline to bound retries.
  `init_cache` carries a `debug_assert!` that the lock is held, so this class of
  mistake fails loudly under `cargo test` instead of degrading into a rare
  ENOENT that only CI sees.

### Verification
- Each defect is covered by a deterministic test that does not depend on
  timing, and each was confirmed to fail against the old behaviour by
  temporarily restoring it:
  `init_lock_survives_the_wipe_it_guards` fails as soon as `wipe_cache_dir`
  goes back to `remove_dir_all`, and
  `give_up_path_still_initialises_under_the_lock` trips the `debug_assert!`
  the moment the give-up path initialises without the lock.
- End to end, `mire test tests -j 8` on a fresh cache directory: the fatal write
  error does not reproduce locally on either build (0 in 6 runs before and
  after), because it depends on how the CI runner's threads interleave. What is
  measurable locally is the redundant-reinitialisation cascade it caused, seen
  as the tolerant `dropping unreadable WAL file` notices: 17-76 per run before
  the fix (mean ~44) against 0-14 after it (mean ~5.7). Those notices are
  themselves intended behaviour — a thread pruning a WAL file another thread had
  already removed — and never fail a build; the drop in their frequency is just
  the cascade going away. CI is the authority on the fatal error, and it is
  green: `Ok: 276 - Passed: 276 - Failed: 0`.

### Added
- Five cache tests in `src/incremental/cache.rs`, all deterministic:
  `init_lock_survives_the_wipe_it_guards` (minimal deterministic reproduction),
  `init_rebuilds_directories_after_a_wipe`,
  `waiter_steals_a_stale_lock_instead_of_returning_uninitialised`,
  `give_up_path_still_initialises_under_the_lock` (forces the last-resort
  branch with the staleness check disabled, and proves via the `debug_assert!`
  that the init still happens under the lock), and
  `waiter_returns_immediately_when_the_cache_is_already_versioned` (guards
  against the fix trading the race for a stall).
- `concurrent_cold_cache_init_preserves_all_writers` in the incremental cache
  tests: eight threads released from a barrier into a brand-new cache
  directory, over several rounds, asserting that no writer's entry is lost or
  corrupted. It guards the invariant but does not by itself reproduce the
  regression — it also passes against the pre-fix code, because the barrier
  sends every thread into the load phase together, so none reaches `save()`
  until the initialisation storm is over.
- `wait_for_cache_init_with` takes the two timing thresholds as arguments, so
  lock stealing and the give-up path are testable without waiting 30 seconds.

### Added
- **`in` is accepted as a name, not only as the membership operator.** The `in`
  keyword is unavoidable — it is the `for`/`find` iterator — but the standard
  library's stdin module is also called `in`, and three separate places rejected
  the collision outright, so `module in`, `load mire::std::in` and `in::line()`
  were each a syntax error. The module declaration and the load path now take the
  wider path-segment rule, and an expression head accepts the keyword, which is
  what makes `in::` work as a namespace like any other. The change is narrow on
  purpose: the `in` operator itself, and `for`/`find` over it, are untouched, and
  a bare `in(1)` still parses as a call named `in` and is left for name resolution
  to accept or reject — that layering is unchanged, only the head widened. Six
  tests in `src/parser/mod.rs` cover the module declaration, the load path, the
  namespaced call, the operator, the two loop forms, and the bare-call case.

### Notes
- Cache tests use `tests/cache/` as their fixture root rather than `/tmp`, so a
  failure leaves its evidence in the test tree. `tests/cache/.gitignore` ignores
  everything the tests create, so nothing they leave behind can be committed.
- The lock is a transient directory used for mutual exclusion inside the
  compiler's own build cache. It is not a lockfile, and Avenys has no lockfile
  handling of any kind: `owl.lock` and dependency resolution belong to Owl
  alone. The compiler is handed source and a normalized config, keeps its cache
  under `bin/.cache`, and compiles. That boundary is now stated explicitly in
  `src/avens/manifest.rs`.

## 4.3.1 - 2026-09-29
A patch release. Two of these are the reason CI stopped passing on a current
stable toolchain, and the third is a filesystem write that reported failure for
something that is not a failure.

### Fixed
- **An unreachable match arm made the build fail on newer stable** — the
  member-access arm in `get_struct_name` was already covered by the or-pattern
  above it, which lists `Expression::MemberAccess { data_type, .. }` and reads
  the struct name straight off the resolved type. The arm after that could
  therefore never run, and rustc now rejects the build for it. The arm is
  removed, so the arm that always ran is the only one. Behaviour is unchanged,
  because the removed arm was dead code.
- **A private method nothing called broke the build as a dead-code warning** —
  `pipeline_input_element_type` had no callers. Removed.
- **Writing an empty string to a file reported failure** —
  `linux_file_write` rejected `length <= 0`, but a zero length is a valid
  no-op that POSIX reports as 0 bytes written rather than as an error. Writing
  an empty file therefore came back as a failed write even though the file was
  created. Only a negative length is rejected now. This mattered because
  `fs::write` in kioto compares the byte count against the length it asked
  for, so a correct zero-byte write was being counted as a mismatch.

> Both build failures were reported by CI, not by the local suite, because the
> local toolchain predates the change in rustc that promoted them to errors.

## 4.3.0 - 2026-09-26
### Added
- **`bits::<T>(x)` bit-level reinterpretation intrinsic** — a same-width scalar
  bitcast, lowered straight to the existing `MirOp::BitCast`, so the emitted IR
  is a native LLVM `bitcast` with no runtime call and no C builtin.
  `DataType::bit_width()` reports the real storage width of plain scalars
  (i8..i128, u8..u128, f32, f64) and `None` for bool, char, str and containers.
  `bitcast_target()` resolves the `bits::T` / `bits.T` namespace suffix to a
  `DataType` and distinguishes "not a bitcast" from "bitcast with a bad target",
  so the type checker can reject a wrong argument count, a non-bitcastable
  target, a non-scalar source and any width mismatch with a spanned diagnostic.
  Tests cover the f64 roundtrip, zero bits, identity, denormal patterns, NaN
  payload preservation and the i32/f32 width pair.

> This is new language surface, hence the minor bump rather than a patch.

### Fixed
- **Float division folded to `0.0` during constant folding** — `SDiv` dispatched
  on an `is_float` flag but its constant-folding arm ignored both operands and
  returned `0.0` for every float division, so a program that divided two
  literals such as `1.0 / 2.0` had the division erased at compile time and
  printed `0`. The arm now folds to `x / y`. Covered by four cases in
  `src/compiler/mir/optimize/mod.rs`: exact halves, signed zero, and operands
  that are themselves constant-folded expressions.
- **A test build inherited the project's artifact** — `mire test` on a project
  declaring `artifact = "shared"` built a shared object, so the runner had no
  executable to run and reported every test file `ok` having executed no
  assertion. It reached every library package, and a green from that path was a
  build success rather than a test result. `normalize_test_build_options()` now
  forces an executable and lifts a `none` runtime tier to `minimal`, once at the
  entry point and before any consumer of those fields branches on them, so the
  guarantee holds however the compiler is driven. A normal build is untouched:
  packages can still publish the artifact they declare.
- **A failing test file could report zero failures** — the summary counts
  `@[test]` declarations, so a script-style test declaring none (a plain `main`
  asserting through a helper) summed to nothing and printed `Failed: 0`
  directly beneath a `FAILED` line. The exit code was always correct, which is
  why it went unnoticed: only the number a human reads was wrong. A failing unit
  now counts as at least one failure, in both the summary and the per-family
  logs.
- **The runner never executed script-style tests, and never noticed a crash** —
  two independent gaps, both of which turned a real failure into a green. A test
  file that declares no `@[test]` and just runs assertions from `main` was only
  compiled, so its assertions were never reached; and a non-golden unit was
  judged solely on its output text, so a test binary that segfaulted printed no
  `[FAIL]` line and was reported as a pass. The exit status is now honoured, and
  a file under a configured test directory, or named explicitly on the command
  line, runs even when it declares no `@[test]`. `should_skip_run()` keeps the
  one case that should not execute: a program in the swept tree that is not a
  test path stays `ok (compiled)` and has no run-time effect, so compiling
  anything cannot turn into a side effect. On this correction four genuine
  compiler bugs became visible for the first time, all previously masked by the
  two gaps above; each is reduced to a self-contained repro and recorded in the
  affected project's tests:
  - a function whose parameter and return type are both `anything` crashes when
    called with a concrete value;
  - the pipeline operator applied to a lambda, `5 => (x => double(x))`, crashes;
  - reading a field of a struct-valued field, `o.inner.value`, returns the wrong
    value when it does not crash outright;
  - a fixed array of structs that carry a managed string field crashes, while
    the same array over a struct with only an integer field is fine.
  A further gap is a type-safety hole rather than a code-generation one: `dasu`
  has type `None` yet lowers to `0`, so an `i64 == str` comparison survives type
  checking and reaches an `inttoptr` plus `strcmp` at run time.
- **`tests/Operators/Pipeline` asserted through `dasu` against a string** — the
  case did not test the pipeline, and the comparison above is what let it
  compile. It now checks the actual result value and adds an independent
  arithmetic pipeline case.

## 4.2.2 - 2026-09-25
### Fixed
- **Compiler ownership bug**: the borrow checker treated `Str` as a non-copy
  type, producing false "Use after move" errors when a string was passed to a
  function such as `strings::concat`. `Str` is now a copy type
  (`src/compiler/borrowck/mod.rs`).

> The release note first published in 4.2.1 announced this fix, but the code
> change only shipped in 4.2.2. The 4.2.1 bump carried no source change.

## 4.2.1 - 2026-09-25
### Documentation
- **PAL API** (`src/pal/pal.h`): documented the ownership conventions
  (`[PAL-OWNED]`, `[BORROWED]`, `[WRITE-INTO]`), the thread-local
  `pal_last_error_message()` error getter, capability-based removal
  `pal_root_remove`, and the safe FFI variants `pal_dir_next_into` /
  `pal_dir_next_name`. Clarified that UNSANDBOXED functions are internal-only.
- Added the 4.2.0 changelog entry.

## 4.2.0 - 2026-09-22
### Documentation
- **PAL API documented**: Added ownership conventions (`[PAL-OWNED]`, `[BORROWED]`, `[WRITE-INTO]`), thread-local error getter `pal_last_error_message()`, capability-based removal `pal_root_remove`, safe FFI variants `pal_dir_next_into` and `pal_dir_next_name`. Clarified UNSANDBOXED functions are internal-only.


### Added
- **Multi-arch installer** (`install/install.sh`): detects the host release
  triple from `uname -m`, overridable via `--arch` / `MIRE_ARCH`, and pulls
  per-architecture release archives (`mire-compiler-<triple>.tar.gz`,
  `owl-<triple>.tar.gz`) with the x86_64 legacy fallback name. Supported
  triples: x86_64, aarch64, riscv64.
- **`--check` (read-only audit)**: validates distro, libc (glibc >= 2.39),
  detected arch, package manager and LLVM (>= 18) and exits 0/1 without
  installing or downloading.
- **`--build-from-source`**: fetches avenys-rust, ensures rustc >= 1.85 and
  builds the compiler against the host LLVM (for older glibc, different libc
  variants, custom LLVM, or architectures without prebuilt archives).
  `SOURCE_URL` / `SOURCE_REF` / `MIRE_RUSTUP_URL` override the defaults.
- **`--docker` fallback**: installs inside the `mire-lang/toolchain`
  container (GNU glibc >= 2.39), for distros without official support.
- **Multi-arch release pipeline** (`.github/workflows/release.yml`):
  x86_64/aarch64 compiler matrix plus an experimental riscv64 build under
  QEMU (greater `continue-on-error`; never blocks the release).
- **Toolchain container** (`docker/toolchain.Dockerfile` +
  `.github/workflows/docker.yml`): publishes a full LLVM/Clang toolchain
  image to GHCR on release.

### Documentation
- README install section rewritten: architecture detection (`--arch`),
  `--check` audit, `--build-from-source`, `--docker`, install options, and
  release archive table.
- All `.md` docs cleaned to standard ASCII/markdown (no emojis or non-ASCII
  symbols), keeping only project-structure trees in tree(1) style.

## 4.1.1 - 2026-09-18

### Fixed
- **`[c]` user cflags are C-only flags**: `c_defs.cflags` (e.g. `-I...`
  include paths forwarded by Owl from a project's `[c] include`) were also
  passed to the `llc` invocations that lower Mire IR. `llc` rejects C driver
  flags and exits immediately, killing the IR pipe (`Failed to stream LLVM IR
  into llc: Broken pipe`). `cflags` now reach only `clang` (C compilation and
  link) and project C object compilation; `llc` is driven solely by the
  configured `--target`/`-mtriple`.

## 4.1.0 - 2026-09-17

### Fixed
- **Ownership / memory leak (runtime)**: the `Drop` of a variable now loads the
  value held by the slot and releases that, instead of freeing the address of
  the stack slot (a silent no-op). Reassigned owned strings and collections are
  now actually freed. As a consequence a growing string is no longer quadratic
  in memory (an internal 10k-iteration concatenation probe dropped from
  ~808 MB to ~1.6 MB).
- **Retain on borrow**: `set y = x` where `x` is an owned pointer now emits
  `rt_managed_retain` for the borrowed source, so the alias stays valid after
  the original variable is reassigned or dropped.
- **Concat operand release**: `rt_string_concat` copies its operands, so fresh
  call-result operands of a string `+` are released after the concatenation.
- **Range arity**: `range`/`to` dispatches by argument count: 1 arg ->
  `rt_math_range_i64`, 2 args -> `rt_math_range_between_i64`, 3 args ->
  `rt_math_range_step_i64`. LLVM declarations added.
- **Void return codegen**: functions and `Ret(None)` now emit `ret void` for
  `()` functions instead of `ret null`-style defaults.
- **`@[attr]` after an expression**: an `at`/`@` token that starts a new line
  is treated as a top-level attribute (`@[test]`), not as an outdented index
  expression.
- **Function-name shadowing**: declaring a local with the same name as a
  function is now a declaration, not a reassignment to the function symbol.
- **Minimal runtime tier / PAL bridging**: when a retained runtime `.c` file
  bridges into PAL at the C level (`helpers.c`, `thread.c`), the PAL sources are
  now compiled in even when the IR symbol scan finds no PAL use.
- **Compiler bugs discovered** (7 real codegen bugs): implicit-return-0,
  `(expr):f64` call-arg, `::`-generic-enum, generic-impl constructors,
  nested-fn captures, bounded skill-dispatch, `=`-labelled args ignoring order.
- **Comment syntax**: documentation comments normalized from `#` to `//` line
  comments (Mire `#` comments are a lexical error). Block comments `/! !/`
  preserved.
- **Type ascription**: removed the invalid `set u = () :mu` unit-type example
  from the types docs; updated the Unit/Void table row description.

### Added
- **`pal_file_chmod` PAL capability**: added the `fs_chmod` field to
  `pal_ops_t`, implemented `linux_fs_chmod` using
  `chmod(path, strtol(mode, NULL, 8))`, and added a WASI stub. Enables
  `fs::permission` in Kioto.
- **File crypto runtime helpers** (`helpers.c`, declared in `runtime.h`):
  `rt_crypto_sha256_file_hex`, `rt_crypto_sha512_file_hex`,
  `rt_crypto_base64_file`, `rt_crypto_ed25519_verify_b64`,
  `rt_crypto_ed25519_verify_file`, and `rt_crypto_ed25519_pubkey_raw_b64`,
  with shared base64 encode/decode and binary-safe file reading.
- **`src/runtime/bytes.c`**: raw buffer allocation, little-endian readers/
  writers and blend helpers extracted from `helpers.c` so the minimal runtime
  tier can use them (math.c, FFI bindings) without the PAL-dependent file.

### Changed
- Documentation reorganized into self-contained sections under `docs/`: ABI,
  CLI, compiler, errors, FAQ, libraries, PAL, runtime, syntax (23 topics) and
  WASM; root changelog and `docs/README.md` index added.
- `owl.toml` build manifest declares `runtime = "full"`.
- Documentation coherence updates across 22 syntax sub-topics to align with
  actual compiler behavior; `collections`, `enums`, `control-flow` and
  `pattern-matching` READMEs had broken syntax claims corrected (unit type,
  type-before-`=`, `len(arr)` on arrays, match `=>` arrows, index-loops,
  find-in-collection).

### Deprecated
- None

## 4.0.0
- Initial release of Avenys v4.0.0 with PAL v4 hardening, range arity dispatch, and documentation coherence initiative.
- **Owl-managed compiler boundary**: the compiler is a restricted interface;
  project discovery, dependencies, registries and lockfiles belong to Owl,
  which passes a generated config with `--config` (`runtime = "minimal"`,
  `target x86_64-unknown-linux-gnu`, `artifact = "bin"` defaults). The old
  `--libt` spelling was removed.
- **LLVM object pipeline**: Mire IR is lowered to native objects with `llc`;
  static libraries use `llvm-ar`, shared libraries use `ld.lld`, executables
  use Clang only for target-aware CRT/libc linking.
- **Explicit build paths**: `--cache-dir`/`--cache` and `--output-dir`, with
  deterministic standalone defaults under `bin/`; `--lib-dir` supports multiple
  search roots and `~` expansion. Package resolution no longer reads the
  consumer's `[dependencies]` table.
- **Shared libraries**: `crate-type = "cdylib"` / `"staticlib"` in `owl.toml`
  (or `--crate-type` on the CLI) produce `.so`/`.a` with all symbols exported.
- **Debug CLI help**: `mire debug --help` documents `--tokens`, `--ast`,
  `--ir`, `--run`, profiles and output options.
- **Load/use enforcement (E0025 / E0026)**: `use!` is forbidden on `load`
  (package) modules (call directly); mandatory on `load!` (local) modules.
- **Runtime tiers documented**: `full`/`minimal`/`none` C-file and library
  sets, with demand-driven minimal compilation.
- **WASM/WASI targets**: `wasm32-wasip1` (WASI SDK sysroot) and
  `wasm32-unknown-unknown` (no-libc freestanding).
- `strings_minimal.c`: POSIX-free string operations for the minimal tier.

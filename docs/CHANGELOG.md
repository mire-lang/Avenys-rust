# Changelog

All notable changes to Mire are documented in this file.

## [3.19.1] - 2026-07-21 (F1.2)

### Changed

- Async channel: replaced file-based IPC (`/tmp` files) with real pthread
  mutex + condition variable message queue. `async::channel()` returns an
  opaque handle; `send`/`recv`/`close_channel` use the handle directly.
  Channel supports blocking recv with a non-blocking queue.

## [3.19.0] - 2026-07-21 (F1)

### Added

- GPU detection: `pal_gpu_snapshot` reads Linux sysfs (`/sys/class/drm/`)
  to enumerate GPUs with vendor, device, driver, and VRAM info. Returns a
  real dict instead of a hardcoded `"available=false"` string.
- Dicts: `dicts.get_i64` reads integer values from dicts. Read-only dict
  operations (`get`, `has`, `len`, `keys`, `values`, `entries`) now take
  `&dict` references instead of moving the dict, allowing multiple reads
  on the same dict.

### Changed

- GPU kioto module returns a parsed dict from the C PAL function directly.
- Dicts module: `rt_dicts_get_i64` C function added for type-safe i64
  retrieval from string-keyed dicts.

## [3.18.8] - 2026-07-21

### Added

- Regression test `tests/vec_indexed.mire` for vector indexed read/write.

## [3.18.7] - 2026-07-21

### Changed

- Moved `11_maps.mire` from `tests/broken_mire/` to `tests/` (now compiles).

## [3.18.6] - 2026-07-21 (F0.3)

### Fixed

- Vector indexed-assignment: adjust Gep index by header offset (8 bytes) for
  `Vector`/`List` types in both read and write paths. Write path resolves target
  type via `var_types` instead of `extract_data_type`.
- Map type annotation: `[] :map[K V]` now produces a Dict instead of a Vector.
  `apply_type_ascription` converts empty List to Dict when the target type is Map.

## [3.18.5] - 2026-07-21

### Changed

- Resolve owl dependencies from `~/.owl/libs` instead of `~/.owl/modules`,
  matching the package layout produced by owl.

## [3.18.4] - 2026-07-21 (F0.4)

### Added

- Built-ins allowlist enforced from the manifest. `MireManifest.builtins`
  (`enabled`, `allow`) lets a project restrict privileged built-ins; the type
  checker rejects calls to built-ins not present in `allow` when enabled. With
  no `[builtins]` section all built-ins remain allowed.

## [3.18.3] - 2026-07-21 (F0.2)

### Added

- Accept `:Type mut` parameter syntax. The `mut` token after a parameter type
  is now parsed and consumed; it exposes the existing capability of treating
  parameters as mutable internally.

## [3.18.2] - 2026-07-21 (F0.1)

### Added

- Parse qualified generic calls, e.g. `helper::push[i64](v 5)`. `parse_postfix`
  now accepts `Expression::MemberAccess` as a call target with type arguments
  (via `member_access_name`), not just `Expression::Identifier`.

## [3.18.1] - 2026-07-21 (F0)

### Added

- `docs/ROADMAP.md`: ecosystem roadmap and versioning scheme (target 4.0.0,
  phases F0–F5 with documentation sub-phases).

### Changed

- Removed `docs/VEC_INDEXED_ASSIGN_BUG.md`; the vec indexed-assignment bug is
  now tracked as part of the F0.3 compiler work in the roadmap.

## [3.18.0] - 2026-07-19

### Fixed

- **Private fn visibility for `load!` modules**: `select_imported_statements`
 in `src/loader/renamer.rs` now runs full dependency resolution when
 `items=None` (wildcard imports), matching the explicit-items path. Private
 `fn` declarations are now discoverable as transitive dependencies via the
 `internal_name` match for `Statement::Function`. Previously, private `fn`
 in `load!`-imported modules were invisible to the dependency resolver,
 causing "Unknown function" errors for owl sub-packages using `load!`.
- **`statement_export_name` visibility consistency**: `Type`, `Skill`, and
 `Enum` variants in `src/incremental/utils.rs` now only export when
 `visibility == Public`. Previously these were always exported regardless
 of visibility, inconsistent with `Function` which already checked
 `Public`.

## [3.17.0] - 2026-07-18

### Added

- **Real fixed-width scalar types (up to 128-bit).** Mire now uses true
 integer widths `i8`/`i16`/`i32`/`i64`/`i128` and `u8`/`u16`/`u32`/`u64`/`u128`,
 32/64-bit floats `f32`/`f64`, `bool`, and `char` (32-bit UTF-32 code point).
 LLVM lowering maps each type to its exact width (`u8`→`i8`, `char`→`i32`,
 `u64`→`i64`, etc.); there is no silent widening to `i64`.
- **Type ascription `(value :T)` / `expr :T`.** A trailing `:T` converts a
 value to the named type. Conversions are checked, not silent:
 - Integer → wider integer (same/larger width): zero-extended (`u*`) or
 sign-extended (`i*`); never loses information.
 - Integer → narrower integer: only allowed when the literal value fits the
 target range; otherwise a compile error (no silent truncation).
 - Integer → float (`f32`/`f64`): allowed.
 - Float → integer: requires an explicit ascription and discards the fraction.
 - Float → float: `f64 → f32` truncates, `f32 → f64` extends without loss.
- **Arithmetic preserves declared widths.** `+ - * / %` compute in a promoted
 width (`i64` for integers, `f32`/`f64` for floats) and narrow the result back
 to the operands' declared type, so `u8 + u8` yields `u8` and `f32 * f32`
 yields `f32`.
- **`print` / `dasu` / `str` resolve by the argument's real type.** Floats use
 `rt_f32_to_string` / `rt_f64_to_string`; 128-bit integers use the new
 `rt_i128_to_string` / `rt_u128_to_string` runtime helpers; narrower integers
 are extended to `i64` for `rt_i64_to_string`.
- **Granular type-conversion error codes.** Real-types conversion failures now
 report dedicated codes: `E0100` (precision loss), `E0101` (type mismatch),
 `E0102` (implicit float→int), `E0103` (signed/unsigned boundary), `E0107`
 (integer literal out of range). See `docs/ERROR_CODES.md`.
- **`load!` / `use!` — simple local module import.** `load! <path>` exposes
 nearly all `pub` content of a relative `.mire` file or directory as a
 namespace (the **last path segment** is the module name; no alias). A
 leading `/` resolves from the project root (the `owl.toml` dir); a bare
 directory defaults to `main.mire` (fallback `mod.mire`). Resolution only
 searches up to **2 levels below the project root**. Unlike package `load`,
 `load!` needs no `owl.toml`, `module` declaration, or `exports` table. Every
 call into a `load!` module **must** be wrapped in `use!`
 (`set x = use! module::symbol(...)`); a bare `module::symbol(...)` is a type
 error. Package `load` (kioto) is unaffected.

### Changed

- **No silent numeric widening or narrowing.** Assigning a wider/narrower or
 differently-signed numeric value now requires an explicit ascription; the
 compiler rejects implicit precision-losing conversions.
- Integer literals infer `i64`; floating-point literals infer `f64`. Select a
 narrower/signed/unsigned type with an ascription (e.g. `200 :u8`).

## [3.16.1] - 2026-07-17

### Docs

- **`docs/VEC_INDEXED_ASSIGN_BUG.md`** — nuevo documento de bug conocido:
 la asignación por índice nativa (`set vec at idx = val`) sobre vectores
 dinámicos `vec[T]` corrumpe el vector silenciosamente (lecturas
 posteriores devuelven valores erróneos/desplazados). Caso mínimo
 reproducible incluido. Workaround: usar `lists::set(vec, idx, val)` de
 kioto (enruta a `rt_lists_set_i64`), que escribe el slot correcto.
 No afecta `arr[T N]` (array fijo) ni `lists::get`/`lists::push`.

## [3.16.0] - 2026-07-15

### Added

- **Warning system overhaul**: Added `WarningFilter::Off` (default, suppresses
 all warnings). New CLI flags `--show-warn` / `--sh-warn` (summary table) and
 `--position` / `--pos` (per-file detailed output). Warnings are collected as
 raw `Diagnostic` values in `BuildResult.warnings_raw: Vec<Diagnostic>` and
 suppressed for test paths in `run`/`build`/`check` commands. Summary table
 aggregates by diagnostic name at the end of test output.
- **Runtime location (Phase A)**: Runtime safety functions (`rt_div_i64`,
 `rt_rem_i64`, `rt_check_bounds_i64`) now accept `i64` line, `i64` column, and
 `ptr` source filename parameters. `rt_panic_loc(msg, line, col, file)` emits
 runtime errors with source position. LLVM declarations in `build_pipeline.rs`
 updated. `lower_program_with_filename` and `mir_to_llvm_with_filename` thread
 source filename through the MIR pipeline. `MirLower.filename` passed to bounds
 check calls in `lower/expr.rs`/`lower/stmt.rs`. `LlvmCtx.source_filename`
 passed to div/rem in `codegen/expr.rs`.
- **`mire build --help` and `mire check --help`**: Now print usage flags
 instead of erroring with "Unknown option: --help".
- **`[tests]` schema supports arbitrary keys**: Nested `read_owl_test_paths` in
 `test_command` now accepts any key-value pair (not just `path`/`dirs`),
 matching the standalone `read_test_roots` function.

### Changed

- **Warnings are now off by default**: `WarningFilter::Off` is the default
 filter. Previously all warnings were shown. Use `--show-warn` to see a
 summary or `--position` for per-file details.

## [3.15.0] - 2026-07-15

### Removed

- **Legacy Avenys (`LlvmIrGen`) codegen backend has been fully removed**. The
 `MIRE_LEGACY_CODEGEN` env var, the `--allow-legacy-known-broken` flag, the
 `BuildOptions::allow_legacy` field, and all `src/avens/llvm_*.rs` modules
 (binary, builtins, collections, control, dicts, functions, helpers, lists,
 strings, types) are gone. MIR is now the **only** codegen path; there is no
 alternative backend and no compile-time branch to select one. Affected
 commands: `mire build`, `mire run`, `mire debug`, `mire test`.

## [3.14.1] - 2026-07-15

### Added

- **CI guard `scripts/check_syntax_version.sh`** fails the build if
 `SYNTAX.md`'s version line or example count drifts from `Cargo.toml` / the
 actual ```` ```mire ```` fenced-block count. `SYNTAX.md:3` was stale
 (`3.11.38 · 287 tests`), now `3.14.1 · 55 examples`.

### Fixed

- **Parser no longer infinite-loops on comma-separated struct/type field lists**.
 In `parse_nominal_type_statement` (`src/parser/statements.rs`), the field loop
 only advanced when the current token was `Ident`; a `,` separator (as in
 `struct P { x :i64, y :i64 }`) was never consumed, so the loop spun forever and
 `mire build`/`check` hung with no timeout (a CI DoS). The loop now consumes `,`
 field separators and, defensively, always makes forward progress on unexpected
 tokens. Verified by a minimal `struct P { x :i64, y :i64 }` program that hung
 before the fix and now builds.

- **MIR backend no longer silently drops module-level `set`/`let` bindings**.
 `lower_program` (`src/compiler/mir/lower/mod.rs`) only lowered
 `Function`/`Impl`/`Extern`/`Struct`/`Enum` statements, so a top-level
 `set p = (P x: 5, y: 3)` was never lowered. The binding's slot was absent, so
 field reads (e.g. `p.x`) folded to `0` and the program printed the **wrong**
 output `0,0` with no error — the legacy backend handled this via entry-block
 allocas. Scalars failed louder (`undefined value '@n'` at link time). The fix
 lowers module-level bindings correctly:
 - **Primitive-typed** top-level `set`/`let` (i64/i32/f64/f32/bool/char/str)
 become **true MIR globals** (`MirProgram.globals`, emitted as
 `@name = global <type> zeroinitializer`), loaded/stored via
 `MirValue::Global`. Because they are globals, they are visible from *any*
 function — not just `main` — so a module/library function can read a
 module-level const (the dangerous case where the const is consumed by a
 function other than `main`, e.g. a `load`ed library).
 - **Aggregate-typed** top-level bindings (structs, vectors, lists, maps) are
 prepended to the `main` prologue as ordinary locals (alloca + store),
 matching legacy for the reported struct-literal case (a bare `store` of an
 SSA aggregate does not initialize a global correctly in this ABI, so
 aggregates stay local).
 - **Loader reachability** (`src/loader.rs`, `select_imported_statements`) now
 also treats private top-level `Let`/`Assignment` as reachable internal names,
 so a private module-level `set` consumed by an imported function is pulled in
 under `load`. Previously a private top-level `set` used by a library function
 was dropped, causing "Avenys unknown identifier".
 Verified by `benchmarks/struct_literal_top_level/run.sh` and `kioto`
 (`mire test`: 78/78). The silent-wrong-output variant is the dangerous one.

- **Negative float literals now lex correctly**. In `src/lexer/mod.rs` the
 `-<digit>` branch always emitted an `IntLit` (with value `"-2.0"`), so negative
 floats failed with `Invalid integer literal '-2.0'` in every context (args,
 list elements, comparisons, `&&`/`||` RHS, struct fields, etc.). The branch now
 emits `FloatLit` when the magnitude contains a decimal point, mirroring the
 positive-number path. Note: negative exponent floats (`-2.0e3`) are still not a
 single token — same pre-existing limitation as positive exponent floats, out of
 scope.

### Changed

- **Legacy Avenys (`LlvmIrGen`) codegen is now disabled by default and rejected
 without explicit opt-in**. The legacy backend is known-broken: it produces
 incorrect numeric/string output and does not support `load kioto` or `;`
 statement separators. It was reachable only via the `MIRE_LEGACY_CODEGEN` env
 var. From this release, setting that env var without the new
 `--allow-legacy-known-broken` flag is a hard error. There is **no** silent
 fallback to legacy; MIR is always the default. Affected commands: `mire build`,
 `mire run`, `mire debug`, `mire test`. (Verified manually:
 `MIRE_LEGACY_CODEGEN=1 mire build ...` now errors with "legacy codegen is
 known-broken... pass --allow-legacy-known-broken" unless the flag is given.)

### Known Issues (environment)

- **Stale local install shadows the current build — layer-8 / process issue, not a
 compiler defect**. During validation we found the installed `mire`
 (`~/.local/bin/mire`, `~/.cargo/bin/mire`) was an older build (v3.12.5) while the
 working tree was v3.14.1, so tests "failed" or behaved differently until the
 binary was rebuilt and reinstalled. The compiler was correct; the divergence was
 purely environment / path-hygiene. **Mitigation:** after pulling compiler
 changes, rebuild (`cargo build --release -p Avenys`) and reinstall `mire` to
 every `PATH` location that shadows it; verify with `mire --version`. The same
 applies to `owl` and the `kioto` module under `~/.owl/modules/`. This is tracked
 as an environment ("layer 8") concern, not a code bug.

### Fixed

- **Final `clang` codegen now respects the selected optimization level**:
 `compile_binary_from_ir` in `toolchain.rs` emitted `clang` with a hardcoded
 `-O0`, discarding the optimization performed by `opt -O3`. The opt level is now
 threaded through `build_pipeline.rs` and applied via `opt_level.as_opt_flag()`,
 so release builds (`-O1`/`-O2`/`-O3`/`Os`/`Oz`) get real machine-code
 optimization. Debug builds are unchanged (`as_opt_flag()` returns `-O0` for
 `O0`). **Blast radius:** affects codegen of every program built at `-O1`..`-O3`.
 This closed a 2.6× runtime gap vs Rust on optimized builds (Mandelbrot
 2000×2000@1000 went from 4.37s to 1.67s, matching Rust, with identical FP
 results — `clang -O3` on unchanged IR is bit-identical without fast-math).
- **`mire test` harness created `bin/debug/test`**: The test runner built each
 test binary at `bin/debug/test/<stem>` but never created the `bin/debug/test`
 directory, so every test failed at the `clang` link step with
 `Not a directory` / `No such file or directory`. The directory is now created
 (and a stale file at that path is removed first) before compilation.

### Added

- **`mire test -O`, `--opt-level <n>`, `-r`/`--release`, `-d`/`--debug`**: Test
 binaries can now be compiled at a chosen optimization level. Previously the
 harness hardcoded `OptLevel::O0`, so the suite could never exercise the
 optimized codegen path. `-r` maps to `-O3`, `-d` to `-O0` (default).

### Validation

- Full kioto test suite run at both `-O0` and `-O3` yields identical results
 (74 passing / 1 failing in both). The single failure is a pre-existing Mire
 limitation (no negative float literals, e.g. `-2.0`), unrelated to this change.
 No precision regression from the codegen change.

## [3.14.0] - 2026-07-13

### Added

- **`mire test --jobs, -j <n>`**: Parallel compilation of test files. When `0`
 (default), uses `std::thread::available_parallelism()`. Test harnesses are
 compiled in chunks of size `n` inside a `std::thread::scope`.
- **`mire test --jobs=N`** syntax: Alternative `--jobs=4` form supported in
 argument parsing.

### Changed

- **`mire test` now compiles in parallel**: All test files are compiled
 concurrently within a thread scope, then executed sequentially. This
 dramatically reduces wall-clock time on multi-core machines.
- **Test harnesses moved to `bin/.cache/test/`**: Instead of writing to
 `/tmp/`, the auto-generated test wrappers live in `bin/.cache/test/<stem>.mire`.
 This ensures `find_project_root()` can locate `owl.toml`, making the
 incremental compilation cache reusable across all tests in a single run.

### Removed

- **Phase 0 dead code**: Debug-dump environment variables `MIRE_DEBUG_CACHE`
 and `MIRE_DEBUG_LINK`, and the `[DEBUG IR]` eprintln from
 `build_pipeline.rs` removed. Error codes `E0002`, `E0004`, `E0006` and 13
 unreferenced warning codes (`W0015`, `W0016`, `W0020`, `W0022`, `W0023`,
 `W0026`–`W0033`) eliminated from `diagnostic.rs`.
- **Redundant `async::exists()`**: Removed from kioto's `core/async/mod.mire`
 — identical to and superseded by `proc::exists()`.

## [3.13.0] - 2026-07-12

### Fixed

- **`ErrorKind::Runtime` now carries source position**: Added `line: usize,
 `column: usize` fields to `ErrorKind::Runtime`, matching every other error
 variant. Previously the only variant without location data, causing all
 runtime errors to display `<source location unavailable>`. All construction
 sites updated with `line: 0, column: 0` defaults.
- **`attach_context` detects `(0,0)` instead of `(1,1)`**: The condition that
 skips attaching context for unpositioned errors was stuck at `(1,1)` —
 leftover from an old line-numbering scheme. Changed to `(0,0)` to match
 the actual sentinel for `NO_POSITION`.
- **External tool errors chain source filename**: `optimize_ir()` and
 `compile_binary_from_ir()` in `toolchain.rs` now accept a `source_filename`
 parameter and call `.with_filename()` on their errors. `build_pipeline.rs`
 passes `&source_filename` at both call sites, so linker/optimizer failures
 include the output path instead of being orphaned.

### Changed

- **Runtime error display**: When no source position is available, the
 compiler now shows `<no source location; emitted from non-positioned
 backend/runtime path>` instead of the misleading `<source location
 unavailable>`, making it clear the error originates outside the MIR/source
 line tracking system.
- **`incremental/mod.rs`**: `StoredErrorKind::Runtime` extended with `line`
 and `column` fields for cache serialization compatibility.

## [3.12.5] - 2026-07-10

### Fixed

- **`str()` builtin shadowed by `get.str`**: When `load kioto` was used, the
 `str()` conversion function was incorrectly resolved to `get.str` (from
 `kioto/lists/get.mire`). `extract_bare_name_map` in `mir/lower/mod.rs` now
 excludes `"str"` from bare-to-qualified mappings, ensuring `str()` always
 calls `rt_i64_to_string`.
- **List literal corruption with negative integers**: `-5349999486874862801`
 inside `[...] :vec[i64]` was lexed as `Minus` + `IntLit`, causing the parser's
 `parse_additive` to treat `a -b` as binary subtraction `a - b`. This collapsed
 3 list elements into 2 and corrupted the second value. The lexer now emits a
 single `IntLit` token with the negative value when `-` is immediately followed
 by a digit (no whitespace).
- **`strings::substr` off-by-one**: Clamping used `(size_t)(start + length) > len`
 which could overflow `int64`. Changed to `length > avail` where
 `avail = (int64_t)(len - (size_t)start)` for safe comparison.

### Docs

- **CHANGELOG.md**: Added missing entries for v3.12.3 and v3.12.4.
- **Benchmark tests**: Updated all `.` → `::` notation in compiler benchmark
 fixtures (`lists.fold` → `lists::fold`, `fs.write` → `fs::write`, etc.)

## [3.12.4] - 2026-07-09

### Added

- **Multi-line expression continuation**: Binary operators at end of line
 trigger implicit continuation. `skip_newlines()` added in all 10 binary
 operator parse functions: `parse_or`, `parse_xor`, `parse_and`,
 `parse_equality`, `parse_bitwise_or`, `parse_bitwise_and`,
 `parse_comparison`, `parse_additive`, `parse_shift`,
 `parse_multiplicative`. Works for: `&&` `||` `+` `-` `*` `/` `%` `<<`
 `>>` `&` `|` `^` `==` `!=` `<` `>` `<=` `>=`.
- **Binary-safe file I/O**: `rt_read_bytes(path)` reads a file in binary
 mode and returns a managed string with correct byte length set in the
 header (no `strlen` truncation on embedded nulls). `pal_fs_read_bytes`
 and `pal_fs_write_bytes` provide PAL-level binary-safe read/write.
- **`rt_hex_to_file(path, hex)`**: C function decodes hex string to raw
 bytes and writes to file. Replaces the `xxd -r -p` subprocess in
 Ed25519 verify — crypto module now has **zero external binary conversion
 dependencies**.

### Changed

- **Install script**: `openssl` added as a prerequisite for crypto module.
 `xxd`/`vim-common` added then removed after `rt_hex_to_file` replacement.
- **CI workflows**: `openssl` installed in `apt` deps for test runners.

### Docs

- **SYNTAX.md**: Updated with bitwise operators (`^ & | << >>`), multi-line
 expression continuation, and list/dict literal quirks.

## [3.12.3] - 2026-07-09

### Added

- **Bitwise operators**: `^` (XOR), `&` (bitwise AND), `|` (bitwise OR),
 `<<` (shift left), `>>` (shift right). New MIR ops: `MirOp::Shr`,
 `MirOp::Xor`, `MirOp::BitAnd`, `MirOp::BitOr`. LLVM codegen emits
 `lshr`, `xor`, `and`, `or` respectively.
- **`rt_lists_set_i64(list, index, value)`**: C runtime function for
 in-place list element mutation (`lists::set`).
- **`rt_crypto_byte_at(str, index)`**: C runtime function for raw byte
 access from managed strings, used by hex/base64 decode.

### Fixed

- **MIR lowering for unknown binary ops**: Previously all unrecognized
 operators silently routed to `MirOp::Add`. Now correctly dispatches
 each bitwise operator.

### Changed

- **Constant folding, DCE, simplify, inline, wrapper passes**: All updated
 to handle the five new `MirOp` variants.
- **Hash discriminants**: byte 27 = `BitAnd`, byte 28 = `BitOr` in
 `mir/mod.rs`.

## [3.12.2] - 2026-07-08

### Fixed

- **Test discovery no longer follows symlinks**: `walkdir()` in `src/main.rs`
 now skips symlinks during test file discovery. Previously, `tests/lib` (a
 symlink to `testlib/`) was recursed into, discovering `tests/lib/mod.mire`
 as a test file. Since that file contains `module` and `load` statements
 (only valid at top level), wrapping it in `pub fn main: () { ... }` caused
 a syntax error when run as a test.

## [3.11.45] - 2026-07-07

### Fixed
- **Analysis cache invalidation**: `analysis_cache_key` now includes `dep_fingerprint`
 (combined hash of all loaded files + dependency edges). Previously, changing a
 loaded module's source would not invalidate the importing file's cached analysis.
- **`store_analysis_error`**: Now writes a `::latest` entry (same as `store_analysis`),
 so partial re-analysis always finds the most recent snapshot even after errors.
- **LRU tracking for `::latest` keys**: Both fingerprint-keyed and `::latest` entries
 are LRU-tracked; `store_analysis` and `store_analysis_error` both maintain the
 latest key.
- **`blob_hash` clone**: Fixed borrow-after-move when computing blob hash for
 analysis entries.
- **Flat builtin aliases**: Both MIR and LLVM codegen now accept flat names
 (`fs_exists`, `fs_read`, `fs_drop`, `fs_list`, `proc_run`, `proc_exit`,
 `env_get`, `env_cwd`, `env_all`, `time_unix_ms`, `mem_used`, `cpu_count`)
 alongside the canonical `::` names. This provides backward compatibility for
 code using the legacy flat style.

### Added
- **`dependency_fingerprint()`**: New utility in `src/incremental/utils.rs` that
 hashes all loaded file paths, content hashes, and dependency edges into a single
 `u64` for the analysis cache key.

### Docs
- **`docs/incremental-design.md`**: Added §"Analysis Cache Invalidation (v3.11.45+)"
 documenting `dependency_fingerprint`, three-param `analysis_cache_key`,
 `latest_analysis_key`, two-entry store, and cache migration.
- **CHANGELOG.md**: Bumped to 3.11.45.

## [3.11.44] - 2026-07-06

### Added
- **Builtin modularization**: Namespace syntax for builtins replacing
 legacy flat names:
 - `thread::spawn` / `thread::join` (was `thread_spawn` / `thread_join`)
 - `time::now::ms` (was `time_unix_ms`)
 - `cpu::count` (was `cpu_count`)
 - `mem::used` (was `mem_used`)
 - `env::get` (was `env_get`)
- **Parser `::` member access**: `check_double_colon()` in parser helpers
 recognizes `::` as member access, building `MemberAccess { target, member }`
 AST nodes. The chain `module::sub::fn(...)` lowers to `module.sub.fn(...)` for
 call resolution.

### Changed
- **Builtin registry**: `default_builtin_returns` updated for all new namespace
 builtins.
- **Kioto adapters**: `core/async/mod.mire` uses `thread::spawn`/`thread::join`
 instead of legacy `thread_spawn`/`thread_join`.

## [3.11.43] - 2026-07-06

### Added
- **`thread_spawn` builtin**: Spawns an OS thread running a closure.
 Internally calls `pal_thread_spawn` via `rt_thread_spawn_closure`
 runtime helper. Supports both closure variables and inline closures.
 Returns `i64` thread ID.
- **PAL thread layer**: `pal_thread_spawn`, `pal_thread_join`,
 `pal_thread_self`, `pal_thread_exit` with pthread implementation
 on Linux. `-pthread` linker flag added automatically.
- **Closure variable calls (`typeck_check_expression.rs:448`)**: When
 a variable name has `Closure` or `Function` type, the call is accepted
 and its return type used, instead of emitting "Unknown function".
- **MIR indirect closure calls (`mir/lower/expr.rs:305`)**: Closure
 variables lower to `MirValue::Temp(ptr)` instead of `FunctionRef`,
 enabling codegen to emit indirect calls via the closure struct.

### Changed
- **Type checker**: `default_builtin_returns` in `src/builtins/mod.rs`
 now includes `thread_spawn` → `I64`.

### Docs
- **CHANGELOG.md**: Bumped to 3.11.43.

## [3.11.38] - 2026-07-03

### Fixed
- **Loader**: `select_imported_statements` in `src/loader.rs` now also matches
 `ExternFunction` and `ExternLib` by their declaration name (not just via
 `statement_export_name`), so private externs are no longer dropped when loading
 modules via `ImportMode::Reachable` with selected items. (Fixes SDL3 function
 resolution when importing `kioto::sdl3`.)

### Changed
- **Type checker**: Removed three `eprintln!` debug lines (`DEBUG_STMTS`,
 `DEBUG_FUNCS`, `DEBUG_SEL`) from `src/compiler/typeck.rs:109-121` that were
 left over from SDL3 FFI debugging.

### Docs
- **CHANGELOG.md**: Bumped to 3.11.38.

## [3.11.37] - 2026-07-01

### Added
- **SYNTAX.md overhaul**: Comprehensive documentation of previously undocumented
 features: generics (functions, structs, enums, impl, trait bounds), closures,
 pipeline operator (`=>`), match guards (`when`), or-patterns and numeric ranges,
 error propagation (`?`), `Box[T]` heap allocation, constant modifier (`set x :i64 const`),
 generic trait bounds, static methods, and higher-order list functions.

### Fixed
- **FFI codegen**: Added `DataType::Pointer(Box<DataType>)` AST variant so FFI pointer
 types (`*mut i8`, `*const i8`) generate `ptr` in LLVM IR instead of `i64`. Void
 return types (`extern fn foo: ()`) now emit `void` instead of `i64`. Both fixes
 resolve SDL3 and other C-library FFI call compatibility.

### Changed
- **FFI compiler**: `parse_ffi_type()` returns `Pointer(inner)` instead of `I64`.
 `DataType::None` maps to `"void"` in extern LLVM declare statements.

## [3.11.36] - 2026-07-01

### Changed
- **Repository reorganization**: install script moved to `install/install.sh`,
 README URLs updated accordingly.
- **CHANGELOG consolidated**: root `CHANGELOG.md` removed; all version history
 is now in `docs/CHANGELOG.md` only.
- **Project renamed**: repository directory renamed from `mire/` to `avenys/`.
- **Build artifacts**: `target/`, `bin/`, and `.cobject_cache/` cleaned from
 version control and local workspace; `.gitignore` entries ensure they stay
 out.

### Fixed
- **Release workflow**: GH release `body_path` now points to `docs/CHANGELOG.md`
 instead of the removed root `CHANGELOG.md`.

## [3.11.35] - 2026-06-26

### Added
- **MIR collections lowering**: `lists.map`, `lists.filter`, and
 `lists.fold` now lower through MIR instead of relying on ad hoc frontend
 handling.
- **`proc.on` plumbing**: wired in MIR codegen, Avenys LLVM codegen, and the
 build pipeline declaration so signal handlers resolve end to end.

### Fixed
- **OWL regression test**: `owl_build_run_info_cycle` is enabled again and
 covered by the test suite.
- **Version metadata**: package/version docs aligned to `3.11.35`.

### Docs
- **MIR pipeline**: updated builtin and collections notes to match current
 lowering/codegen behavior.

## [3.11.32] - 2026-06-24

### Fixed
- **dasu dict render leak**: `emit_dasu_expr` now frees the rendered string
 after printing Dict/Map types via `rt_managed_free`.
- **split_list raw malloc leak**: `rt_strings_split_list` now uses
 `rt_managed_from_cstr` and `rt_managed_from_slice` (managed) instead of
 `rt_strdup_raw`/`rt_strdup_raw_n` (raw malloc).
- **Managed string lookup performance**: replaced O(n) linked-list scan in
 `rt_managed_contains` and `rt_managed_unregister` with an open-addressing
 hash table for O(1) amortized lookup. Linked list retained for iteration
 in `rt_managed_cleanup_all`; hash table keys freed on cleanup.

### Docs
- **SYNTAX.md**: Added `Avenys` maintainer identity note.

## [3.11.31] - 2026-06-24

### Fixed
- **String concatenation memory leak**: LLVM codegen now tracks and frees
 intermediate temporaries produced by chained `+` operations. In long-running
 programs (e.g. HTTP servers), chained concat previously leaked every
 intermediate result until memory exhaustion. Fixed in both MIR and Avenys
 codegen paths with `owned_temps`/`owned_string_temps` tracking sets and
 `rt_managed_free` insertion.
- **Dict value overwrite leak**: `store_value` now frees old string values
 when a dict key is overwritten. Previously only `store_key` freed old keys.
- **Dict remove leak**: `rt_dict_remove` now frees old key+value strings
 at the removed slot before swapping in the last entry.
- **MIR PAL name mismatch**: `builtin_to_pal` mapped `fs_dir`/`fs_name`/`fs_ext`
 to `pal_fs_dirname`/`pal_fs_filename`/`pal_fs_filext` (nonexistent). Fixed to
 correct PAL names `pal_fs_dir`/`pal_fs_name`/`pal_fs_ext`.
- **proc_exec ownership flag**: marked result `owned: true` (was `false`).
 `pal_proc_exec` returns raw `malloc`'d output; the flag now correctly
 triggers managed copy in `store_variable`.

### Added
- **`rt_list_free`**: frees list backing allocation.
- **`rt_dict_free`**: frees all STR keys, STR values, dict storage arrays,
 and the dict struct itself.
- **`rt_closure_env_free`**: frees closure environment allocations.
- **`rt_managed_cleanup_all`**: drains the managed-string linked list at
 program exit, freeing all registered string headers and list nodes.
- **Avenys function-exit cleanup**: `emit_string_locals_cleanup` iterates
 `self.vars`, loads and calls `rt_managed_free` for every variable with
 `owns_heap_string = true`. Called before explicit `return` statements
 and implicit end-of-function returns.
- **`declare i32 @fflush` / `declare void @rt_managed_free`** added to
 MIR builtin declarations.

### Docs
- **SYNTAX.md**: Added comprehensive §7 Skills (Traits) section with
 declaration, implementation, self-type annotation, multiple impls,
 generic trait bounds, instance vs associated methods, and error examples.
 Section numbering updated through §20.
- **SYNTAX.md**: Version bumped 3.11.28 → 3.11.31.

## [3.11.30] - 2026-06-24

### Added
- **Kioto modularization**: `net` now exposes `net::http::get`, `net::http::post`,
 `net::connect::timeout`, `net::recv::all`, `net::socket::nonblock`.
 `ws` exposes `ws::send::text`, `ws::recv::all`.
 `async` exposes `async::task::done`, `async::task::error`.
 All underscore-separated functions replaced with namespace-separated `::`.
- **Memory safety**: MIR codegen now wraps all `pal_*` extern string returns
 with `rt_managed_from_cstr() + free()` — strings returned from C no longer
 leak. Previously PAL strings were raw `malloc` pointers never freed.
- **`pub extern fn`**: ExternFunction now has a `visibility` field. `extern fn`
 without `pub` is private to the module (not exported). `pub extern fn`
 exports the symbol. This prevents FFI leaks — only `pub fn` wrappers are
 exposed to downstream consumers.
- **Parser**: `pub`/`priv` now accepted before `extern fn` statements.

### Changed
- **Removed `Statement::Use`/`UseModule`**: `use` always produces an expression
 statement (side-effect call). Import is exclusively via `load`. This
 eliminates the confusing dual semantics of `use`. Removed `resolve_use_path`,
 `parse_use_path`, `parse_use_module_name`, `peek_after_double_colon_chain`.
- **WASM removed**: `pal_wasm_*` PAL functions, `src/pal/linux/pal_wasm.c`,
 `src/pal/wasm/pal_wasm.c`, and `kioto/ext/wasm/` deleted. Server-side
 HTTP/HTTPS/WS modules planned as replacement.
- **`peek_after_double_colon_chain`** removed from parser helpers (unused after
 Use disambiguation removed).

### Fixed
- Memory leak: all PAL string returns now properly managed through
 `rt_managed_from_cstr` wrapping.

## [3.11.29] - 2026-06-22

### Added
- PAL integration tests: `pal_env_get_returns_home`, `pal_env_cwd_returns_non_empty`,
 `pal_fs_write_read_roundtrip`, `pal_fs_path_ops_join_dir_name_ext`,
 `pal_fs_mkdir_rmdir`, `pal_proc_shell_echo`, `pal_proc_spawn_wait_exit_code`.
- OWL integration test: `owl_build_run_info_cycle` verifies `owl build`,
 `owl run`, and `owl info` against a known project.
- `proc.pipe`: joins commands with ` | ` and runs via `pal_proc_shell`.
- `proc.on`: basic signal handler registration via new PAL function `pal_proc_on`.
- `proc.err`: returns captured stderr from last process via `pal_proc_err`.
- `proc.run`/`proc.spawn`/`proc.exec`/`proc.exec_bg`: now join args with the
 command string instead of ignoring arguments.
- New `task` module (renamed from `async`): `kioto/core/task/` provides the
 same API; `async` module now delegates to `task` and is deprecated.

### Fixed
- `pal_proc_run` now captures stderr via temp file in addition to stdout.
- `pal_proc_shell`/`pal_proc_exec` both use the stderr-capturing path.

## [3.11.28] - 2026-06-18

### Fixed
- Trivial dereference: `&T → T` where both map to `ptr` at LLVM level (e.g. `str`,
 `list`, `map`) no longer emits a spurious `load ptr, ptr`. This fixes SIGSEGV
 in kioto wrapper functions like `env.get`, `env.cwd`, `env.args` that use
 `*key` to unwrap `&str → str`.
- Standalone file loading: `mire check/build file.mire` without an `owl.toml`
 project now resolves imports (e.g. `load kioto`) instead of only parsing the
 source file. The `load_shallow_program` fast-path was replaced with a full
 `ImportResolver` rooted at the source file's parent directory.

## [3.11.27] - 2026-06-16

### Infrastructure
- First build marker: initial packages published to mire-lang/libs (kioto, owl)
- Build-based compatibility boundary established for Avenys B1.001
- libs repo: https://github.com/mire-lang/libs.git

## [3.11.26] - 2026-06-16

### Fixed
- Closure capture codegen: insertion order of capture GEP instructions corrected
 (moved before `lower_function_body` so captured variables are visible to the
 closure body as local variables).
- Closure env struct types now propagated to `MirProgram.struct_types` so LLVM
 GEP can resolve the struct layout.
- Removed dead-code warnings (5 unused functions/imports across hashing, semantic,
 and typeck modules).

## [3.11.25] - 2026-06-15

### Changed
- MIR codegen: extern-function wrappers are now generated on demand only for
 extern functions that are actually referenced as values (direct calls, `call`
 builtin targets, or stored in function-typed variables). This significantly
 reduces LLVM IR size and link time for programs that import kioto's large
 extern-function surface.

### Fixed
- Restored MIR function inlining (was temporarily disabled during profiling).

## [3.11.24] - 2026-06-15

### Added
- MIR: first-class function-value foundation. `MirValue::FunctionRef` is now
 emitted for closures and resolved to a concrete `@fn_...` symbol, and every
 mire function (including generated closure functions) receives an implicit
 `env_ptr` parameter so indirect calls can pass a null environment pointer.
- MIR lowerer: closure literals are now lowered into standalone functions
 (`closure_N`) instead of being expanded inline at every `call(...)` site.
- MIR lowerer: `lists.map`, `lists.filter`, and `lists.fold` are now lowered
 into loops that call the supplied closure function for each element.
- MIR codegen: wrappers are generated for `extern fn` declarations so they
 share the mire `env_ptr` calling convention and can be used as function
 values. (Fixes `callback_call_extern_function_value_alias_runs_end_to_end`.)

### Fixed
- MIR inliner: parameter mapping now substitutes `MirValue::Param` with the
 caller's argument value and no longer allocates duplicate parameter slots,
 so inlining small functions (including short closures) produces correct IR.
- MIR codegen: any block whose terminator is still `Unreachable` after
 lowering/inlining now emits a default return instead of LLVM `unreachable`,
 preventing control from falling through into blocks appended later (e.g.
 inliner continuation blocks).

### Tests
- Full regression suite is green: 140 passed / 0 failed.

## [3.11.23] - 2026-06-15

### Fixed
- Kioto `core/dicts`: `rt_dicts_get()` and the `get` wrapper now declare a
 `:str` return type so the runtime's `void*` result is propagated instead of
 being discarded as a void call. (Fixes
 `kioto_async_ready_value_compiles_and_runs`.)

## [3.11.22] - 2026-06-15

### Fixed
- MIR lowerer: bare `len(...)` is now dispatched to the correct runtime length
 helper based on the argument type (`rt_strings_len`, `rt_list_len`, or
 `rt_dicts_len`) instead of relying on ambiguous bare-name resolution that
 could pick `strings.len` for a list. (Fixes
 `borrowck_moves_in_if_else_are_tracked_per_branch`.)
- Build pipeline: added LLVM declarations for `rt_strings_len()` and
 `rt_dicts_len()`.

## [3.11.21] - 2026-06-15

### Fixed
- MIR caching: `MirFunction::compute_hash()` now hashes function signature, all
 instruction opcodes, operands, constants, types, and terminator values. The
 previous implementation only hashed block IDs and result-temp IDs, causing
 the MIR program cache to return stale IR when source changes altered only
 literals or constants. (Fixes
 `incremental_recompile_keeps_enum_match_string_result_consistent`.)

## [3.11.20] - 2026-06-15

### Fixed
- Type checker: `contains(...)` and `strings.contains(...)` on non-string
 collections now emit a `Backend` error instead of silently compiling to an
 unimplemented call. (Fixes
 `backend_rejects_unimplemented_contains_instead_of_returning_silent_false`.)

## [3.11.19] - 2026-06-15

### Fixed
- MIR lowerer: void calls no longer allocate a result temp. A call whose
 return type is `()` now emits a `None`-result `Call` instruction and yields
 `Const(None)`. This prevents undefined-value errors when a void call is used
 in a return expression (e.g. `return rt_dicts_get(...)` in kioto wrappers).
- MIR codegen: result temps allocated for instructions without an explicit MIR
 result are now offset into a high-ID space (`%t100000+`) so they can never
 collide with MIR-defined temps like `%t0..%tN`.
- Runtime safety: integer division and modulo now call `rt_div_i64()` /
 `rt_rem_i64()`, which panic with "division by zero" on zero divisors. (Fixes
 `runtime_division_by_zero_exits_with_error`.)
- Runtime safety: array/list indexing now calls `rt_check_bounds_i64()` before
 the access, which panics with "index out of bounds" when the index is outside
 `[0, len)`. (Fixes `runtime_out_of_bounds_exits_with_error`.)

### Added
- Runtime: `src/runtime/safety.c` with `rt_panic_division_by_zero()`,
 `rt_panic_out_of_bounds()`, `rt_div_i64()`, `rt_rem_i64()`, and
 `rt_check_bounds_i64()`.
- Build pipeline: LLVM declarations for the new runtime safety helpers.

## [3.11.18] - 2026-06-15

### Fixed
- MIR lowerer: dict literal rendering — `dasu()` / `str()` / `print()` now wrap
 map/dict arguments with `rt_dict_to_string()` so nested maps print as strings
 instead of raw struct bytes. (Fixes `nested_map_string_render_executes_-
 without_runtime_errors`, `syntax_reference_prototype_compiles_and_runs`.)
- MIR lowerer: nested map `value_kind` — dict literals with map values now call
 the new runtime helper `rt_dicts_set_with_kind()` so the runtime knows the
 value is a map and can recursively stringify it.
- MIR lowerer: signed integer modulo — `%` was falling through to `Add` because
 `SRem` was missing from the MIR op set. Added `MirOp::SRem`, lowered `%` to
 it, and wired it through codegen, inlining, and optimization passes. (Fixes
 `signed_integer_division_and_remainder_match_runtime_expectations`.)
- MIR lowerer: `for` loop list element indexing — list layout is `[len, elem0,
 elem1, ...]`, but the loop used the raw index as the GEP index, reading the
 length field as the first element. The index is now offset by 1. (Fixes
 `secondary_for_loop_binding_compiles_and_uses_index`.)
- Runtime: `rt_strings_split()` rewrote empty-segment handling; it no longer
 uses `strtok()` (which drops empties) and now appends a trailing empty
 segment when the input ends with the separator. (Fixes
 `strings_split_preserves_empty_segments`.)
- MIR lowerer: inline closure calls — `call((x) => ..., ...)` with a closure
 literal callee now lowers the closure body inline in the caller, binding
 parameters and preserving captured variables. (Fixes
 `callback_call_closure_with_capture_runs_end_to_end`.)
- Type checker: generic nominal type argument parsing — `Box[T]` now parses `T`
 as `DataType::Generic("T")` instead of `Unknown`, so instance method dispatch
 can bind generic parameters to concrete types. (Fixes
 `generic_impl_method_codegen_builds_for_concrete_type`.)
- MIR codegen / build pipeline: `DataType::Generic` now maps to `i64` in LLVM IR
 so generic struct fields and method returns use a concrete scalar type.

### Added
- Runtime: `rt_dict_to_string()` and `rt_dicts_set_with_kind()` helpers.
- MIR op: `SRem` for signed remainder.

### Changed
- Test suite: added `struct_types: HashMap::new()` to `MirProgram` initializers
 in unit tests so the test target compiles.
- Compiler is now warning-free (unused `program`/`method` parameters fixed).

## [3.11.17] - 2026-06-14

### Fixed
- MIR lowerer: `self` parameter type in impl methods — `self` was parsed with
 `DataType::Unknown` but the lowerer needs `StructNamed(type_name)` for
 `get_struct_name` to resolve field accesses. Now overridden during method
 lowering. (Fixes `impl_method_can_mutate_self_field_and_run`,
 `implicit_self_method_return_still_runs`.)
- MIR lowerer: instance method dispatch — `Expression::Call` with dot-qualified
 names (e.g. `p.distance()`) now resolves the receiver variable's type, looks
 up the method in `method_map`, rewrites the call target to the qualified
 function name (`Point.distance`), and prepends the receiver as the first
 argument. (Fixes `instance_method_call_resolves_and_compiles`.)

### Added
- MIR program metadata: `MirProgram.method_map` field (`HashMap<String,
 HashMap<String, String>>`), extracted from `Statement::Impl` in
 `extract_method_map()` and passed to `MirLower` for instance method dispatch.

### Changed
- MIR lowerer: match arm pattern handling — `EnumVariantPath` patterns now
 match against discriminant (previously fell through to `_ => Br(cs)` which
 always selected the first arm). (Fixes
 `enum_match_without_default_returns_second_variant_string`.)
- MIR codegen: `EnumNamed` data type maps to `i64` in LLVM IR instead of `ptr`,
 fixing return type mismatch when functions return enum types.

## [3.11.16] - 2026-06-14

### Fixed
- MIR codegen: enum type mapping — `EnumNamed` data types now map to `i64` in
 LLVM IR, matching the discriminant representation, eliminating type mismatch
 errors (`ret i64` vs `ptr`) when functions return enum types.

### Added
- MIR program metadata: `MirProgram.enum_types` field (`HashMap<String,
 Vec<(String, usize)>>`), extracted from `Statement::Enum` in
 `extract_enum_types()` and passed to both lowerer and codegen.
- MIR program metadata: `MirProgram.bare_to_qualified` field (`HashMap<String,
 String>`), extracted from IR qualified name map in `extract_bare_name_map()`,
 enabling bare-name to IR-qualified name resolution.

### Changed
- MIR lowerer: `Expression::EnumVariantPath` now returns the correct integer
 discriminant instead of `Const(Int(0))`, and match arm patterns for both
 `EnumVariant` and `EnumVariantPath` compare against the real discriminant.
 (Fixes `enum_match_without_default_returns_second_variant_string`,
 test count 118/140.)
- MIR lowerer: bare-name resolution — `Expression::Call` resolves unqualified
 function names (`main`) to their IR-qualified symbols (`fn_main`) via the
 `bare_to_qualified` map before emitting the `Call` op.

## [3.11.15] - 2026-06-14

### Fixed
- MIR lowerer: `Expression::Match` lowering rewritten — pattern literals now
 use `BrCond` with `ICmp (Eq, match_val, literal)` branching to the matching
 case block or the next check block, and case bodies are lowered into their
 own blocks (not the default block). Dead code / infinite loop eliminated.
 (Fixes `match_expression_with_default_infers_string_branch_type_and_compiles`,
 test count 111/140.)

## [3.11.13] - 2026-06-13

### Fixed
- MIR codegen: temp ID space collision — `tmp_extra` (`%e` prefix) and
 `tmp_result` (`%t{mir_id}`) now use separate counters, preventing MIR
 result temps from aliasing LLVM extra temps.
- MIR codegen: struct metadata pipeline — `struct_types` collected from
 `Statement::Type` during lowering and threaded through `MirProgram` /
 `MirLower` / `LlvmCtx` so field lookup works in codegen.
- MIR codegen: GEP fallback now uses `struct_name` directly as LLVM type
 (instead of hardcoded `"ptr"`), plus `struct:` prefix for explicit struct
 types, enabling array element GEP via element-type strings.
- MIR codegen: `Trunc` op now emits `trunc <src> <val> to <dst>` instead of
 falling through to the no-op catch-all.
- MIR codegen: `Array` and `Slice` data types mapped to `[N x T]` / element
 type in `llvm_type_str`, enabling stack-allocated arrays of the correct size.
- MIR codegen: `And`, `Or` on `i1` values now work correctly (no `icmp ne`
 double-negation).
- Type checker: `referenced_type_for_expr` now unwraps `Ref { inner }` /
 `RefMut { inner }` when falling back to `lookup_var`, fixing `*value`
 returning the reference type instead of the referenced type for function
 parameters of `&T` type.

### Added
- MIR lowerer: `Expression::Index` lowered as GEP + Load (array reads).
- MIR lowerer: `Expression::Reference` lowered as pointer return (skips Load).
- MIR lowerer: `Expression::Dereference` lowered as Load from pointer.
- MIR lowerer: `Expression::UnaryOp` lowered (negation `-` as `Sub(0, x)`,
 logical not `!` as `ICmp(Eq, x, false)`).
- MIR lowerer: `Expression::List` for `Array` data type — stack-allocates the
 array and initializes each element (used in `let x = [a b c] :arr[T N]`).
- MIR lowerer: `Statement::For` lowered as while-loop with iterator alloca,
 index counter, `rt_list_len` call, GEP + Load for element access, and index
 variable registration.
- MIR lowerer: `Statement::Unsafe` lowered by forwarding body statements.
- MIR lowerer: `AssignmentTarget::Index` — GEP + Store for array writes
 (e.g. `arr at i = val`).
- MIR lowerer: `AssignmentTarget::Field` — load struct heap pointer, GEP +
 Store for struct field writes (e.g. `obj.field = val`).
- MIR lowerer: `get_target_elem_type()` helper to extract the LLVM element
 type from a variable's `Array`/`Vector`/`Slice` data type.
- MIR lowerer: `llvm_elem_type_str()` standalone function for LLVM type
 strings from `DataType`.
- Struct metadata: `MirProgram.struct_types` field (`HashMap<String,
 Vec<(String, DataType)>>`), extracted from `Statement::Type` in
 `extract_struct_types()` and passed to both lowerer and codegen.

## [3.11.12] - 2026-06-04

### Fixed
- MIR codegen: struct constructor functions (`@Stack`, `@str`, etc.) are
 generated automatically from `Statement::Type` definitions, enabling struct
 field initialization in the MIR pipeline.
- MIR codegen: `__if_expr` builtin expanded to proper MIR control flow
 (Alloca + BrCond + Store/Load across then/else/end blocks), fixing
 `if`-expression codegen in the MIR path.
- MIR codegen: `@main` entry-point wrapper (`define i32 @main(i32, ptr)`)
 emitted when `@fn_main` is defined, so the MIR pipeline produces runnable
 binaries instead of linker errors.
- MIR codegen: runtime declarations (`@dasu`, `@.fmt_*` globals, `@.argc`,
 `@.argv`) auto-inserted when absent, fixing "use of undefined value" errors
 for built-in I/O.
- MIR codegen: `@dasu` declaration check uses `"declare @"` prefix instead of
 bare `"@dasu"` to distinguish declarations from call sites.
- MIR lowerer: `new_block()` now uses `self.func.blocks.len()` instead of
 `self.next_block`, fixing block ID mismatch that caused `__if_expr` blocks
 to alias the entry block and be eliminated by the optimizer.
- `compile_binary_from_ir` in toolchain: pass object files before `-x ir -`
 so clang auto-detects `.o` files instead of trying to parse them as LLVM IR.

### Added
- Hierarchical load paths: `load kioto::math::basic` resolves through
 `owl.toml [exports]` recursively. `Statement::Load.path` is now `Vec<String>`.
- `module <name>` declaration for intra-package identity (block-style body
 removed — was dead code).
- `use <name>` (bare identifier, no parens) for linking submodules within a
 package via `[exports]`.
- `owl.toml [exports]` section maps export names to `.mire` files or
 sub-directories with their own `owl.toml` for hierarchical resolution.
- `owl.toml [bootstrap]` section with `BootstrapConfig { std_package, std_entry }`,
 defaulting to `kioto` / `"mod.mire"`.
- `MireProject` fields `name`, `version`, `entry` made `#[serde(default)]`
 for minimal manifests. `#[serde(alias = "owl")]` on `project` field for
 backward compat.
- `mire validate` command: validates `owl.toml` dependencies + exports.
- `mire owl add <name> --path <p>|--version <v>`: adds a dependency entry.
- `mire owl remove <name>`: removes a dependency entry.
- `load_exports()` / `resolve_export_path()` in manifest module.
- `resolve_package(name)` in loader: resolves solely through manifest
 `[dependencies]` — no filesystem probing.
- `resolve_load_path(&[segments])` chains through `[exports]` recursively.
- Kioto unbundled: `src/modules/kioto/` deleted; code lives in standalone
 `../mire-kioto/` package with its own `owl.toml` and per-submodule `owl.toml`
 files.
- 10 new parser tests for hierarchical load, `module`, `use` disambiguation.

### Changed
- Module loading is now purely package-based: `load` only resolves through
 `owl.toml [dependencies]`. No heuristic filesystem resolution, no OWL_HOME
 probing, no relative paths.
- `Statement::Load.is_local` removed from AST — one resolve path only.
- `Statement::Module { name, body }` simplified to `Statement::Module { name }`.
- `import` keyword removed entirely from lexer, parser, AST, and CLI.
 Legacy `import` fails to parse with "unexpected token" error.
- `--allow-legacy-imports` flag, `allow_legacy_imports` warning config, and
 `MIRE_ALLOW_LEGACY_IMPORTS` env var removed.
- `mire import` CLI command removed. Use `owl add` / `owl remove` instead.
- `load std` standardized to `load kioto` everywhere.
- `load ./path` is invalid syntax (removed completely, not just warned).
- `owl.toml` manifests normalized to `[project]` section (no duplicate `[owl]`).
- Loader reduced from 1920 to 1561 lines: 7 heuristic functions removed,
 3 new resolution functions added.
- `mire-kioto/` submodules converted to `module <name>` + `use <name>`.
- `mire/owl.toml` created with `kioto = { path = "../mire-kioto" }`.

### Fixed
- Module loading now rewrites internal references inside prefixed modules, so
 recursive calls and cross-references remain valid after namespacing.

## [3.11.10] - 2026-06-02

### Added
- Regression coverage for recursive `load X` modules and root-level local
 module discovery.

### Fixed
- Module loading now rewrites internal references inside prefixed modules, so
 recursive calls and cross-references remain valid after namespacing.
- Local `load` diagnostics now describe the actual `load` path instead of the
 legacy wording.

## [3.11.9] - 2026-06-02

### Changed
- Public manifest dependency types were renamed from `MireImports` / `MireImportEntry`
 to `MireDependencies` / `MireDependency`, with `load_manifest_dependencies`
 as the new loader helper name.
- Documentation in Mire and `mire-docs/` was aligned with the current `load`
 keyword, `--owl-home`, and `[dependencies]` naming.

## [3.11.8] - 2026-06-02

### Added
- `load helper` now auto-discovers local root modules from the project root
 using direct-file and module-directory candidates.
- Regression coverage for root-level module discovery and legacy import
 deprecation warnings.

### Changed
- `import` remains as a legacy alias for `load`, with analyzer support for
 deprecation warnings and the `--allow-legacy-imports` escape hatch.

## [3.11.7] - 2026-06-02

### Added
- New `load` module keyword, with `import` retained as a legacy alias during
 the migration window.
- `--allow-legacy-imports` CLI flag to silence legacy import warnings when
 working with old sources.

### Changed
- Module loading is now surfaced as `Statement::Load` in the compiler AST and
 analysis pipeline, with legacy imports tracked separately for warnings.
- Built-in module documentation now calls out `load` as the preferred source
 keyword.

## [3.11.6] - 2026-06-02

### Fixed
- Bundled Kioto resolution no longer hijacks `import kioto: (...)`; local
 `kioto` modules now win again while `std` keeps using the package-aware
 bundled/Owl lookup.

## [3.11.5] - 2026-06-02

### Added
- Package-aware Kioto resolution now prefers the configured Owl cache root via
 `--owl-home` before bundled fallbacks, and `mire test` honors the same flag.
- `owl.toml` manifests now serialize dependencies under `[dependencies]` while
 still reading older `[imports]` manifests for backward compatibility.
- Regression coverage for Owl-home package resolution.

### Changed
- `ImportMode::Legacy` and the `--import-mode` CLI flag were removed; the build
 pipeline now uses the single reachable import mode everywhere.
- Kioto module resolution now routes `std`, `strings`, `lists`, `math`, and
 the other bundled submodules through package-aware lookup instead of the old
 `std_entry_path()` / `resolve_module_path()` fallback chain.
- `mire import` writes `[dependencies]` in `owl.toml` instead of `[imports]`.

## [3.11.4] - 2026-06-02

### Added
- Kioto `math` core module now split into submodules (`basic`, `stats`, `random`)
 with a shared `_externs.mire` for runtime extern declarations.
- Runtime C helpers for `rt_math_random`, `rt_math_random_range`.
- Multi-level namespace resolution in the type checker: names like
 `math.complex.new` are now resolved by iteratively stripping namespace
 prefixes (`math.complex.new` → `complex.new` → `new`) until a match is
 found in the flat function table.

### Changed
- `src/modules/kioto/ext/iter/mod.mire` rewritten to lower through `rt_lists_*`
 externs directly instead of calling `lists.len`, avoiding a name-resolution
 collision between the `lists` and `strings` modules in certain import
 configurations.
- `decimal.mire` now uses `rt_strings_*` externs directly for string
 manipulation (strip, substr, index_of, pad_left), removing its dependency
 on `import ./../strings` and the associated name-resolution instability.

### Fixed
- `rt_string_to_i64` implemented in `strings.c` and declared in `runtime.h`,
 fixing a linker error when `decimal.mire` is loaded.
- `strip_root_namespace` in the type checker now iterates across multiple
 dot-separated prefixes instead of stopping at the first level, so
 e.g. `kioto::fs::read` resolves correctly even after intermediate
 namespace components are stripped.

## [3.11.3] - 2026-06-02

### Added
- Kioto `strings` and `lists` now use reference-based read paths for shared
 bindings, with runtime C helpers for `strings.index_of`, `strings.repeat`,
 `lists.contains`, `lists.index_of`, `lists.reverse`, and `lists.unique`.
- Regression coverage for Kioto reference APIs on `strings` and `lists`.
- Manifest dependencies now serialize as `[dependencies]` with backward
 compatibility for existing `[imports]` manifests.

### Changed
- `strings.repeat` now lowers through `rt_strings_repeat` instead of an inline
 Mire loop, avoiding reference-vs-value type drift in the wrapper layer.
- `lists` read-only wrappers now borrow `&list` / `&vec[i64]` where appropriate
 so repeated reads do not consume the source binding.
- CLI import resolution now routes Kioto through package-aware resolution with
 `--owl-home` support and no `legacy` import mode.
- Kioto module docs now reflect the runtime-backed `strings` and `lists`
 surface.

## [3.11.2] - 2026-05-31

### Added
- Kioto `math` now executes through real runtime/PAL-backed wrappers for sum,
 mean/avg, variance, stddev, median, range, trigonometric functions, powers,
 constants, and rounding helpers, without changing syntax.

### Fixed
- `math.sum` no longer lowers through a stale Avenys special-case.
- Top-level numeric helpers (`float`, `pow`, `round`, `floor`, `ceil`) now map
 to real math runtime functions instead of identity-style stubs.
- `math` module docs and regression tests now cover the live ABI surface.

## [3.11.1] - 2026-05-31

### Added
- Kioto `async` module with task-result helpers and process-backed spawn/join
 wrappers, without adding or changing language syntax.
- `mire import --json` for CI/editor tooling.
- PAL process spawn API (`pal_proc_spawn`) for Linux, with a safe WASM stub.

### Fixed
- Crate/package version now matches the documented 3.11 series.
- CLI help now reads the crate version at build time instead of a stale literal.
- PAL process declarations now use consistent `i64` PIDs across LLVM, C headers,
 Linux, and WASM.

## [3.11.0] - 2026-05-30

### Removed
- `kioto_abi.c` (643 lines) deleted — all `@mire_*` LLVM symbols renamed.
- Dead declarations removed: `mire_list_new`, `mire_strings_split`, `mire_option_wrap`.

### Changed
- Every `@mire_*` LLVM IR symbol renamed to `@rt_*` (runtime core) or `@pal_*`
 (platform layer). The only `@mire_*` left is `@mire_main`, the user entry
 point. 76+ mappings catalogued in `abi_map.toml`.
- Codegen in all 8 `llvm_*.rs` files updated: declarations and call sites.
- `strings.c` extended with 14 new `rt_*` implementations migrated from kioto_abi.c
 (contains, replace, replace_first, starts_with, ends_with, substr, pad_left,
 pad_right, trim, split_list, join, read_line, get_args, time/cpu elapsed_ms_str).
- `kioto_exports.c` created as a temporary shim (`__kioto_*` → `rt_*` / `pal_*`),
 replacing the old kioto_abi.c. Will be deleted once std/ modules call
 `rt_*` / `pal_*` directly.
- ABI migration registry: `abi_map.toml` at project root documents every
 symbol rename and its category (runtime / pal / removed).
- Build pipeline unchanged — `build_pipeline.rs` already compiled all `.c`
 files from `src/runtime/` and `src/pal/linux/`.
- Crate version bumped to `3.11.0`.

### Added
- PAL documentation in `PAL.md` updated with current architecture, directory
 layout, full function tables for runtime core and PAL, and ABI map reference.

## [3.10.0] - 2026-05-30

### Added
- Kioto ABI v1 closed: all `__kioto_*` extern functions declared in `std/` and implemented in C.
 - 3 new C wrappers: `__kioto_lists_first`, `__kioto_lists_last`, `__kioto_lists_is_empty`.
 - `__kioto_strings_strip` extern added to `std/strings/mod.mire`.
 - Filled remaining proc externs: `run`, `exec`, `shell`, `wait`, `kill`, `exit`, `exists` with function bodies.
- TOML-based import system: `owl.toml` now supports `[imports]` section.
 - New CLI command: `mire import <module> [--version <ver>] [--path <path>]`.
 - `ImportResolver` checks manifest imports before filesystem resolution.
 - New types: `MireImports`, `MireImportEntry` (Simple, WithPath, PathOnly).
 - New functions: `write_manifest`, `load_manifest_imports`.
- `kioto/` modules marked as deprecated — std/ is the canonical module source.

### Changed
- `std/proc/mod.mire` now links to Kioto ABI C externs.
- `MireManifest` extended with optional `imports: MireImports` field.
- Import resolution order: manifest imports > project local > owl home > bundled.

## [3.9.1] - 2026-05-26

### Fixed
- `__kioto_dicts_merge` fixed: was iterating `a` entries instead of `b`, and accessing non-existent `entries[i].key_kind`.
- `__kioto_cpu_loadavg` replaced `getloadavg()` with `/proc/loadavg` read (removes `_GNU_SOURCE` dependency).
- Cleaned redundant `extern` declarations in `__kioto_time_elapsed_ns` / `__kioto_cpu_elapsed_ns`.

### Added
- `match` advanced pattern support (compiler-internal, no syntax breaks):
 - or-patterns (`p1 | p2`)
 - guard patterns (`when <bool-expr>`)
 - numeric range patterns (`a..b`, inclusive)
- Parser support for `result[T E]` and `result[T]` type surface.

### Changed
- Match pattern bindings/validation now understand wrapped patterns used by guards and or-patterns.
- Avenys match lowering now evaluates guard/or/range conditions directly during branch selection.
- Internal `DataType::Result` now stores both `ok` and `err` channels (`Result { ok, err }`), with `result[T]` defaulting `err` to `str`.

### Tests
- Added regressions:
 - `match_supports_or_patterns_and_numeric_ranges`
 - `match_guard_when_is_supported`
 - `match_guard_requires_bool_condition`
 - `parser_accepts_result_type_with_two_slots`
 - `parser_accepts_result_type_with_default_error_slot`

## [3.8.13] - 2026-05-25

### Changed
- Borrow checker now enforces move semantics on pass-by-value call arguments for non-copy types (e.g. `vec`, `map`, `str`, enums/structs), rejecting later use-after-move.
- Incremental build cache keys/fingerprints now include `import_mode` (`legacy|reachable`) to isolate cache reuse paths between global import strategies.

### Tests
- Added regression `borrowck_moves_non_copy_value_when_passing_by_value`.
- Callback regression subset remains green after ownership hardening.

### Performance
- Re-ran `benchmarks/p0_import_reachability_bench.sh` after cache-key isolation:
 - `legacy` warm avg: `55.75ms`, `10018KB` RSS peak
 - `reachable` warm avg: `36.00ms`, `9266KB` RSS peak

### Apps
- `mire-apps` compile sweep now passes `26/26` after syntax/typing cleanup in enum/map/flow samples.

## [3.8.12] - 2026-05-25

### Changed
- Type checker now rejects `call(...)` when callback is typed as `:function` but signature metadata cannot be inferred (instead of compiling with unresolved `Unknown` contract).
- Callback e2e matrix was tightened: dynamic opaque callback cases are now expected compile errors.

### Fixed
- Added backend defense-in-depth for `call(...)` dynamic lowering:
 - reject identifier callbacks without inferable signature metadata,
 - reject dynamic callback lowering when return type remains unresolved.

### Tests
- `language_regressions` callback subset updated:
 - valid: named/external/closure/function-value alias callbacks,
 - invalid: opaque `:function` param/return-value paths without inferable signature.
- Re-ran `benchmarks/p0_import_reachability_bench.sh`:
 - `legacy` warm avg: `47.75ms`, `9966KB` RSS peak
 - `reachable` warm avg: `55.75ms`, `9275KB` RSS peak
 - `reachable` lowers memory footprint in this run, with a small wall-time overhead on warm incremental rebuilds.

## [3.8.11] - 2026-05-25

### Added
- CLI flag `--import-mode legacy|reachable` (default `legacy`) for controlled rollout of global import reachability.

### Changed
- In `reachable` mode, `import modulo` (global, sin selección) now infers used symbols from importer dependencies and only loads reachable exports plus required private transitive dependencies.
- Build pipeline propagates `import_mode` into loader resolution, enabling compile-time pruning without syntax changes.

### Tests
- Added regression `global_local_import_reachable_mode_loads_only_used_symbols` (compares `legacy` vs `reachable` behavior).

## [3.8.10] - 2026-05-25

### Changed
- P0 reachability completada para imports selectivos: `import x: (...)` ahora mantiene cierre transitivo de dependencias intramódulo, evitando arrastrar exports no usados y preservando dependencias privadas requeridas.
- Loader usa grafo de dependencias de statements para construir el set alcanzable por símbolo importado.

### Added
- Regresión `local_import_selected_symbol_keeps_private_dependencies` para garantizar que imports selectivos incluyan helpers privados requeridos por símbolos públicos.

## [3.8.9] - 2026-05-25

### Added
- Modularización P6: `src/compiler/borrowck/mod.rs` (1480→1319 lines). Extraído `check_expression` + `expression_location` a `borrowck_expressions.rs` (166 lines).

## [3.8.8] - 2026-05-25

### Added
- Modularización P5: `src/incremental/mod.rs` (1595→951 lines). Extraída serialización binaria a `serialize.rs` (507 lines) y utilidades a `utils.rs` (141 lines).

## [3.8.7] - 2026-05-25

### Added
- Modularización P4: `src/compiler/typeck.rs` (1814→1236 lines). Extraído `check_expression` (~580 líneas) a `typeck/typeck_check_expression.rs` (586 lines).

## [3.8.6] - 2026-05-25

### Added
- Modularización P2: `src/avens/llvm_functions.rs` (2559→1782 lines). Extraídos 55 métodos `compile_*` + helpers de closure a `llvm_builtins.rs` (776 lines).
- Modularización P3: `src/avens/llvm_collections.rs` (2203→1380 lines). Extraídas operaciones de lista (10 métodos) a `llvm_lists.rs` (583 lines) y de dict (5 métodos) a `llvm_dicts.rs` (250 lines).

## [3.8.5] - 2026-05-25

### Added
- Modularización P1: `src/parser/mod.rs` (3387→749 lines). Extraídos `statements.rs` (598), `expressions.rs` (1432), `helpers.rs` (434).
- Kioto ABI v1 Fase 2A: wrappers `__kioto_lists_*` y `__kioto_dicts_*` en `runtime_support.c` (10+8 funciones C).
- Módulos Kioto `lists.mire` y `dicts.mire` expandidos con wrappers ABI (len, push_i64, concat, remove, delete, clear).
- New builtin dispatchers en `infer_collection_call` para `lists.pop`, `lists.first`, `lists.last`, `lists.is_empty`, `lists.append` con inferencia de tipos genérica correcta.
- LLVM codegen directo para `lists.pop`, `lists.first`, `lists.last`, `lists.is_empty`, `lists.append`.
- Symlinks `src/modules/kioto/*.mire` → `mire-kioto/modules/*.mire`.

### Fixed
- `unify_types`/`is_assignable` ahora manejan `DataType::None` (funciones sin `:Type` podían fallar con "return type mismatch").
- `check_function_statement` trata `None` como `Unknown` para inferencia de tipo de retorno.
- Backend LLVM registra `extern fn` en `user_functions` (antes declaraba pero no generaba `call`).

## [3.8.4] - 2026-05-23

### Changed
- Compiler modularization phase 2:
 - parser import parsing moved to `src/parser/imports.rs`
 - parser type parsing and generic type-parameter helpers moved to `src/parser/types.rs`
 - type checker builtin registry/import helpers moved to `src/compiler/typeck/typeck_builtins.rs`
- `src/parser/mod.rs` and `src/compiler/typeck.rs` reduced as orchestration layers while preserving behavior.

## [3.8.3] - 2026-05-23

### Changed
- Compiler modularization phase 1 (separation of responsibilities, no syntax/semantics changes):
 - parser lifecycle parsing moved to `src/parser/lifecycle.rs`
 - parser pipeline self-placeholder logic moved to `src/parser/pipeline.rs`
 - type checker return-flow helpers moved to `src/compiler/typeck/typeck_returns.rs`
- Updated planning documentation to reflect completed modularization phase and next extraction steps.

## [3.8.2] - 2026-05-23

### Changed
- Removed legacy AST/runtime variants and dead compiler branches:
 - `Statement::{Class, Trait, Code, AddLib, DmireTable, DmireColumn, DmireDlist}`
 - `MireValue::{Object, Trait, Instance}`
- Cleaned parser/type checker/borrow checker/semantic model/backend/incremental paths to stop matching and hashing removed legacy nodes.
- Simplified and updated internal tests that depended on legacy class/code units.
- Updated planning docs (`todo.md`) with current completion status and remaining high-impact performance tasks.

## [3.8.1] - 2026-05-23

### Changed
- Parser cleanup for syntax coherence:
 - removed `none` keyword parsing; `mu` is now the only unit literal/type keyword.
 - removed user-facing parsing paths for legacy `trait` and `code` statements.
- Lexer now reports `?` as an explicit reserved/unsupported token instead of silently keeping a dormant token path.
- Documentation alignment pass:
 - `SYNTAX.md` updated to use `skill` examples and `mu` type reference.
 - `docs/deprecated-cleanup.md` rewritten to reflect current real status (done vs internal pending).
 - `todo.md` updated with active focus on `Performance & Optimizations` and `Quality of Life`.

## [3.8.0] - 2026-05-22

### Changed
- Kioto and Owl code paths migrated to canonical namespace syntax using `::`.
- Parser namespace member recognition extended to support keyword-like member names in paths
 (e.g. `env::set`, `dicts::set`) without conflicting with statement parsing.
- Version bump: `3.7.0` -> `3.8.0`.

## [3.7.0] - 2026-05-21

### Added
- Namespace call compatibility for Rust-style module paths:
 - parser accepts chained `::` calls like `kioto::fs::read(...)`
 - type checker resolves root-qualified aliases to imported module symbols when needed
 - backend call resolution mirrors the same alias fallback for codegen compatibility

### Changed
- Module namespace syntax is now dual-compatible:
 - recommended: `module::submodule::fn(...)`
 - backwards-compatible: `submodule.fn(...)`
- Added parser and language regression tests for double-colon namespace calls.
- Version bump: `3.6.0` -> `3.7.0`.

## [3.6.0] - 2026-05-18

### Changed
- Generic call validation is now strict for non-generic functions:
 - explicit type arguments on non-generic functions now produce a type error
 - avoids silent acceptance of invalid generic syntax at call-site
- Added regression coverage for the non-generic explicit type-args rejection path.
- Version bump: `3.5.0` -> `3.6.0`.

## [3.5.0] - 2026-05-18

### Added
- Backend monomorph call symbol wrappers for generic calls:
 - call-site type arguments now produce stable specialized LLVM symbols
 - wrappers are emitted once per signature and forward to canonical function bodies

### Changed
- Generic nominal normalization in backend (`Type[T]` / `Type[i64]` -> `Type`) for:
 - struct constructor lookup
 - struct field/member resolution
 - impl method metadata and dispatch (`b.get()` on generic nominal receivers)
- Added E2E regression for generic impl method codegen/build path.
- Version bump: `3.4.0` -> `3.5.0`.

## [3.4.0] - 2026-05-18

### Added
- Generic impl method resolution for concrete receiver types:
 - `impl[T] Box[T] { fn get: (self) :T { ... } }`
 - calls like `b.get()` now resolve correctly when `b` is `Box[i64]`.
- Nominal generic type propagation in constructor typing (`Box[i64](...)` keeps concrete nominal type).

### Changed
- Member access/type checking now normalizes nominal generic owners when resolving fields/methods.
- Additional regression coverage for generic impl method resolution.
- Version bump: `3.3.0` -> `3.4.0`.

## [3.3.0] - 2026-05-18

### Added
- Generic `impl` headers in syntax:
 - `impl[T] Box[T] { ... }`
- Generic trait bounds in function type parameters:
 - `fn print_it[T: Show]: (x :T) { ... }`
- Type checker enforcement for generic bounds:
 - validates declared bound trait existence
 - rejects call sites where inferred/explicit generic type does not satisfy bound

### Changed
- AST expanded:
 - `Statement::Function.type_param_bounds`
 - `Statement::Impl.type_params`
 - `Statement::Impl.type_param_bounds`
- Incremental hashing updated to account for new generic metadata.
- Version bump: `3.2.0` -> `3.3.0`.

## [3.2.0] - 2026-05-18

### Added
- Nominal generic type support in parser/type checker:
 - generic type declarations: `type Box[T] { ... }`, `enum Option[T] { ... }`
 - generic nominal type usage in annotations: `Box[i64]`, `Option[i64]`
 - generic constructor/variant call paths:
 - `Box[i64](...)`
 - `Option[i64].Some(...)`
- Type checker generic substitution for:
 - constructor field type validation
 - enum payload validation and match payload bindings

### Changed
- AST expanded:
 - `Statement::Type.type_params`
 - `Statement::Enum.type_params`
- Version bump: `3.1.0` -> `3.2.0`.

## [3.1.0] - 2026-05-18

### Added
- Function-level generics syntax in parser/type system:
 - generic declarations: `fn identity[T]: (x :T) :T { ... }`
 - explicit call arguments: `identity[i64](42)`
 - inferred call arguments: `identity("ok")`
- Generic type node support in AST (`DataType::Generic`).
- Type checker generic argument resolution for function calls (explicit + inferred) with
 consistency validation.
- Parser support for `find` keyword and lowering was retained stable while extending syntax.

### Changed
- `Expression::Call` now carries `type_args` in AST.
- `Statement::Function` now carries `type_params` in AST.
- Incremental hashing updated to include generic function parameters and call type arguments.
- Version bump: `3.0.0` -> `3.1.0`.

## [3.0.0] - 2026-05-18

### Added
- `find` control-flow statement is now fully implemented end-to-end:
 - lexer keyword support (`find`)
 - parser support (`find <item> in <iterable> { ... }`)
 - backend lowering support (no longer rejected as backend limitation)
- New regression coverage for:
 - `find` parse/lower/compile path
 - `const` bindings + compound assignment analysis path

### Changed
- Version bump: `2.9.0` -> `3.0.0`.

## [2.9.0] - 2026-05-18

### Added
- Lifecycle syntax parsing and analysis surface:
 - `new::(...)`
 - `own::(...)`
 - `move::(...) to target`
 - `drop::(...)`
- New AST variants for lifecycle operations: `Statement::New`, `Statement::Own`.
- New warning diagnostic codes `W0028`–`W0033` for explicit ownership guidance in Owl/check workflows.

### Changed
- Removed legacy `vec![T]` type syntax support from parser; canonical vector type is now `vec[T]`.
- Migrated in-repo Mire sources/docs to `vec[T]` notation.
- Extended lexer keyword set for lifecycle and ownership helpers: `new`, `own`, `move`, `drop`.
- Incremental hashing/dependency tracking now covers lifecycle statements.
- `match` validation now rejects duplicate enum arms and enforces exhaustive coverage for enum matches without `_`.
- `match` parser now rejects multiple default (`_`) arms and rejects cases declared after default.
- Lifecycle type rules hardened:
 - `new::` now validates stack-construction targets (`arr/vec/map`).
 - `own::` now validates heap-allocatable targets and reports precise type errors.
- Version bump: `2.8.0` → `2.9.0`.

## [2.8.0] - 2026-05-11

### Changed
- Diagnostic precision hardening extended across type checker, borrow checker and backend context propagation.
- Warning anchoring now prioritizes real source positions and suppresses non-source/internal diagnostics to avoid misleading `1:1` reports.
- Improved compiler/Owl integration stability by surfacing and fixing multiple real ownership and symbol-collision issues discovered from Owl full-build workflows.
- Version bump: `2.7.0` → `2.8.0`.

## [2.7.0] - 2026-05-11

### Added
- New backend optimization level model (`OptLevel`) with `-O0/-O1/-O2/-O3/-Os/-Oz`.
- New roadmap document: `docs/roadmap.md` consolidating completed compiler work and pending Owl integration.

### Changed
- CLI reduced to four commands only: `run`, `build`, `check`, `debug`.
- Default compilation profile switched to `debug` (`-O0`) for faster feedback.
- Build fingerprint now includes optimization level to guarantee cache correctness.
- LLVM `opt` and `clang` flags are now driven by selected optimization level.
- Borrow checker ownership diagnostics now report contextual line/column based on active statement/expression instead of defaulting to `1:1`.
- Type checker/backend diagnostic propagation now reanchors default-position errors to active AST context when available.
- Warning diagnostics (`unused variable/function/import`) now resolve concrete source positions more reliably and skip non-source internal symbols instead of emitting misleading `1:1`.
- Version bump: `2.6.0` → `2.7.0`.

## [2.6.0] - 2026-05-11

### Added
- Unified diagnostic system (`src/error/diagnostic.rs`, `src/error/format.rs`):
 - `Diagnostic` struct with `Severity`, `DiagnosticCode` (E0001–E0015, W0001–W0027), `Label`, `Suggestion`.
 - `format_diagnostic()` with source context, colors, labels, notes, help.
 - `WarningFilter` enum (`Default`, `All`, `Codes`).
- New CLI command `mire check <file>` for analysis-only mode without binary generation.
- New CLI flags: `--warn-all`, `-W <Wxxxx>`, `--deny <Wxxxx>`.
- `analyze_program_with_warnings()` in pipeline with `WarningConfig` (filter + deny).
- Warning tests in `tests/warnings/`.
- Integration guide `docs/owl-diagnostics.md` for Owl tooling.

### Changed
- `MireError` refactored to wrap `Diagnostic` (backward-compatible API).
- `MssError` mapped to diagnostic codes E0007–E0013.
- Warnings rewritten to emit `Diagnostic` instead of `Warning` struct.
- Unused variable/function tracking now works (previously silent).
- `BuildOptions` now includes `warning_filter` + `deny_warnings`.
- Docs updated: `docs/cli.md`, `docs/diagnostic-system.md`, `MORE/0004-diagnostic-system.md`.
- Version bump: `2.5.6` → `2.6.0`.

## [2.5.6] - 2026-05-10

### Fixed
- Updated integration test `extern_and_inline_asm_declarations_parse_and_compile` to use assembly templates compatible with real LLVM inline asm emission.

### Changed
- Patch semver bump: `2.5.5` -> `2.5.6`.

## [2.5.5] - 2026-05-10

### Added
- Runtime string helpers in `src/avens/runtime_support.c`:
 - `mire_strings_contains`
 - `mire_strings_substr`
 - `mire_strings_repeat`
 - `mire_strings_pad_left`
 - `mire_strings_pad_right`
 - `mire_list_pop_i64`
- Backend handlers in `src/avens/mod.rs` for:
 - `list.pop`
 - `contains` / `strings.contains`
 - `strings.substr`
 - `strings.pad_left`
 - `strings.pad_right`
 - `strings.repeat`

### Changed
- Hardened string memory operations (`concat`, `append_owned`, `replace`) with overflow guards and safer capacity math.
- `for` lowering now supports list/vector/slice iteration in addition to `range(...)`.
- `match` pointer comparisons now distinguish string vs non-string pointer semantics:
 - strings -> `strcmp`
 - struct/enum/pointer values -> pointer equality
- Backend now accepts `extern fn` declarations by emitting LLVM `declare` signatures.
- Backend now emits minimal inline `asm` via LLVM `asm sideeffect`.
- Warning set expanded with missing warning codes (`W001`, `W006`, `W010`, `W013`, `W034`, `W036`, `W037`, `W039`, `W043`-`W051`).

### Fixed
- `sqrt(...)` lowering now calls `libm` `sqrt(double)`.
- `Drop` and `Move` statements now lower with concrete backend behavior.

### Added
- Created `std.mire` - Standard Library de Mire con todas las funciones estándar organizadas por categorías:
 - MATH: abs, min, max, sum, clamp, range, round, floor, ceil
 - LISTS: len, push, pop, append, remove, delete, clear, join, contains, index_of, first, last, slice, concat, flatten, reverse, sort, unique, is_empty, map, filter, fold
 - STRINGS: upper, lower, strip, split, replace, contains, startswith, endswith, len, trim, ltrim, rtrim, substr, pad_left, pad_right, repeat, is_empty
 - DICTS: len, keys, values, has, get, set, remove, delete, entries, merge, is_empty
 - TIME: unix_ms, unix_ns, since_ms, since_ns, mark, elapsed, elapsed_ms, elapsed_ns, sleep_ms, sleep_ns
 - TERM: style, hr, clear
 - MEM: used, total, free, available, percent, process, snapshot, format
 - CPU: time_ns, time_ms, mark, elapsed, elapsed_ms, elapsed_ns, count, freq_mhz, cycles_est, loadavg, snapshot
 - GPU: available, snapshot
 - FS: read, write, append, exists, size, copy, move, drop, list, mkdir, rmdir, join, dir, name, ext
 - ENV: get, set, all, args, cwd, chdir
 - PROC: run, spawn, pipe, shell, read, write, on, exit, err, exec, exec_bg, kill, wait, exists

### Changed
- Repository hygiene improvements: standard `.gitignore` added to prevent committing build artifacts (`target/`, `bin/*`, `benchmarks/build/`).
- Removed tracked generated artifacts from git index (`target/`, `benchmarks/build/`, `bin/debug`, `bin/release`) to keep repository lean.
- Syntax documentation standardized into a single canonical file: `SYNTAX.md`.
- `README.md` rewritten for current project status and clearer onboarding.
- Incremental cache format updated to `v5` to reduce in-memory and on-disk metadata duplication.
- Incremental cache blob store now auto-compacts when sparse to avoid unbounded growth under frequent invalidations.
- `stable_statement_hash` now hashes in streaming mode (no intermediate JSON `Vec<u8>` allocation).
- Type checker source context no longer relies on thread-local state; diagnostics now use explicit checker context.

### Fixed
- Backend lowering coverage expanded in Avenys (`src/avens/mod.rs`):
 - Frontend-only statements now handled explicitly as no-op in codegen instead of generic backend failure (`Type`, `Skill`, `Code`, `Class`, `Trait`, `Impl`, `Enum`, `AddLib`, `Module`, `Dmire*`, `Query`, `Find`, `Drop`, `Move`).
 - Added lowering for literal compound forms in `compile_expr`: `Literal::List`, `Literal::Dict`, `Literal::Tuple`.
 - Added qualified string builtin wiring: `strings.contains`, `strings.concat`, `strings.len`, `strings.strip`, `strings.ltrim`, `strings.rtrim`, `strings.is_empty`.
 - Improved unknown function diagnostics from "does not yet lower call" to explicit "unknown function".
 - Expanded `map_type` support for previously unmapped frontend types (`Function`, `Db`, `Datetime`, `Box`, `DynTrait`, `Result`) by mapping to pointer backend representation.
 - Expanded runtime kind classification for struct/enum/function/result families.
- Removed unused constant in incremental cache module (`src/incremental.rs`) to keep warnings clean.
- Simplified enum top-level scan patterns in parser (`src/parser/mod.rs`) by collapsing nested match/if branches without behavior changes.
- Reduced unnecessary `Program` cloning in loader parse/cache path (`src/loader.rs`).
- Added borrow-check regression coverage for impl-method local-reference escapes.
- Lexer token column tracking now preserves start-column for compound operators, improving diagnostic accuracy.
- Runtime CPU MHz cache is now thread-safe via C11 atomics.
- Parser and closure-lowering paths now propagate numeric/closure compilation errors instead of silently defaulting to zero values.
- Avenys numeric lowering now emits real floating-point arithmetic/comparison (`fadd/fsub/fmul/fdiv/frem/fcmp`) when operands are float-typed.
- Type checking now preserves nested vector element types across assignability checks and rejects incompatible inner element pushes (for example `vec[vec[i64]]` + `vec[str]`).
- `for item, index in range(...)` is now fully supported end-to-end (parser/AST/scopes/type-checking/backend lowering).
- Lexer/parser now support prefixed integer literals (`0b`, `0o`, `0x`) and normalize them safely to integer values.
- Raw string literals with hash delimiters are now supported (`r"..."`, `r#"..."#`, `r##"..."##`).
- Added `char` type and character literals; chars are represented as Unicode scalar values (`u32`) in the type system.
- Added frontend support for `unsafe { ... }` blocks (lexer/parser + semantic/type/borrow integration) and backend lowering by compiling contained statements.
- Added frontend support for `extern lib ...` and `extern fn ... lib ...` declarations, including FFI pointer-shape parsing (`*const/*mut`) as scalar `i64` in the current type model.
- Added frontend support for `asm { ... }` blocks and AST preservation; current Avenys backend accepts them as no-op (no target-specific IR emission yet).
- Extended Unicode case conversion: `to_upper`/`to_lower` now handle full Latin-1 supplement range (0xC0-0xFF).
- Fixed memory leak in dict format: nested map values now properly copied instead of returning managed pointer directly.
- Reference mutability semantics: `&x` now infers mutability from original binding (`mut` → mutable ref, otherwise shared), explicit `&mut x` rejected for immutable bindings.

## [2.2.0]

### Added
- Incremental compilation cache improvements (binary cache container, LRU pruning, partial analysis reuse).
- Broader support across structs, enums, methods, collections, and diagnostics.

### Changed
- Avenys backend is the active compiled path.
- Significant maturity improvements in type checking and ownership checking.

## [2.0.0]

### Changed
- Breaking syntax update from v1.x.
- Explicit `self` required for instance methods.
- Associated/static methods use `Type::method(...)`.
- Enum vs impl path behavior clarified.

## [1.0.3]

### Added
- Struct support and method dispatch improvements.
- Field access and enum payload matching fixes.

## [1.0.0]

### Added
- First stable syntax family and compiler baseline.

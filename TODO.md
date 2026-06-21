# Avenys Compiler — Roadmap

> State at B1 release (v3.11.28, modularization complete).

## Priority Legend
- 🔴 **Critical** — breaks correctness or silently produces wrong code
- 🟡 **High** — missing features users notice daily
- 🔵 **Medium** — quality-of-life, hardening, completeness
- ⚪ **Low** — nice-to-have, stretch goals

---

## 🔴 Critical

### C1. Codegen silent fallthrough (`_ => return vec![]`)
- **File**: `src/compiler/mir/codegen/expr.rs:467`
- **Problem**: `MirOp` variants not matched in `compile_inst` silently produce empty LLVM IR. Affects `PtrToInt`, `IntToPtr`, `BitCast`, `Phi`, `Select`.
- **Fix**: Either implement codegen for these ops or emit a compile error instead of silent no-op.

### C2. `contains` on lists/dicts is explicitly unimplemented
- **File**: `src/compiler/mir/codegen/expr.rs` (fallback path); tested by `backend_rejects_unimplemented_contains`
- **Problem**: `lists.contains()` and `dicts.has()` return a "Backend Limitation" error. Users expect this to work.
- **Fix**: Implement `rt_list_contains` / `rt_dict_contains` in C runtime and wire through MIR.

### C3. Runtime C ABI has no integrity tests
- **Problem**: No test verifies that the LLVM IR calling convention matches the C runtime signatures for `rt_list_*`, `rt_dicts_*`, `rt_strings_*`, etc. A mismatch would produce segfaults or wrong results silently.
- **Fix**: Add ABI smoke tests that call each runtime function from Mire and verify results.

---

## 🟡 High

### H1. No tests for PAL operations
| Group | Tests | Risk |
|-------|-------|------|
| `fs_*` (read, write, copy, move, delete, list, mkdir, rmdir, exists, is_dir, size, join, dirname, filename, ext) | 0 | Untested — could be broken |
| `proc_*` (run, exec, spawn, waitpid, kill, exit, exists) | 0 | Untested |
| `env_*` (get, set, cwd, all, args) | 0 | Untested |
| `math` (sqrt, pow, round, floor, ceil, abs) | 2 indirect | Minimal coverage |

### H2. `lists.map` / `filter` / `fold` are stubs in kioto
- **File**: `~/.owl/modules/kioto/core/lists/mod.mire`
- **Problem**: The MIR lowering supports HOFs on lists (`lower/expr.rs:840–1275`) but kioto's `lists.map()`, `lists.filter()`, and `lists.fold()` return `[]` or `0`.
- **Fix**: Wire kioto's HOF wrappers to the built-in lowering path.

### H3. `proc.pipe`, `proc.on`, `proc.err`, `proc.exec` are stubs
- **File**: `~/.owl/modules/kioto/core/proc/mod.mire`
- **Problem**: Return empty strings or alias to simpler operations. Full process pipeline (pipe, signal handling, stderr capture, exec) not implemented.

### H4. Async module is process-spawn in disguise
- **File**: `~/.owl/modules/kioto/core/async/mod.mire`
- **Problem**: No actual async/await, no scheduler, no coroutines. `async.spawn()` is `proc.run()` with a map wrapper.
- **Fix**: Either implement a lightweight async runtime or rename to make the process-spawn nature explicit.

### H5. No integration test for OWL compilation
- **Problem**: The compiler can build OWL, but there is no CI test that runs `owl build`, `owl run`, `owl info` against a known project and checks the output.

---

## 🔵 Medium

### M1. Two backends — legacy LlvmIrGen vs MIR codegen
- **Files**: `src/avens/` (legacy) vs `src/compiler/mir/codegen/` (MIR)
- **Problem**: Two LLVM codegen paths coexist, controlled by `MIRE_LEGACY_CODEGEN` env var. The legacy path (`avens/llvm_*.rs`, 17 files) is abandoned but not removed.
- **Fix**: Remove legacy backend once MIR codegen is proven equal or better.

### M2. Modularization of remaining large files
| File | Lines | Suggested split |
|------|-------|-----------------|
| `src/avens/build_pipeline.rs` | 685 | `build_pipeline/{mod,cache,compile,link}.rs` |
| `src/loader.rs` | 1,676 | `loader/{mod,imports,resolve,parse}.rs` |
| `tests/language_regressions.rs` | 3,763 | `tests/{regressions,borrowck,typeck,parser,kioto}/mod.rs` |
| `src/parser/expressions.rs` | 1,536 | `parser/expr/{prefix,infix,postfix,pipeline}.rs` |
| `src/parser/statements.rs` | 686 | `parser/stmt/{decl,control,import}.rs` |

### M3. GPU module is fake
- **File**: `~/.owl/modules/kioto/core/gpu/mod.mire` (4 lines)
- **Problem**: `available()` returns `true` unconditionally; `snapshot()` returns an empty map.
- **Fix**: Either implement real GPU querying through PAL or remove the module until ready.

### M4. No tests inside kioto itself
- **Problem**: Kioto has 0 test files. All kioto testing is done externally via `mire-owl`. Bug fixes or regressions in kioto require the OWL test suite to catch them.
- **Fix**: Add inline tests in kioto submodules using test directives.

### M5. `owl.toml` exports for async are minimal
- **File**: `~/.owl/modules/kioto/owl.toml` and `core/async/owl.toml`
- **Problem**: Async exports only 2 functions (`ready`, `spawn_join`) but the module has 9 `pub fn`s.
- **Fix**: Audit export lists against actual `pub fn` declarations.

### M6. Documentation: error codes need help messages
- **Files**: `src/error/mod.rs`
- **Problem**: Many error codes lack `default_help_for_code()` messages (only E0005, E0006, E0014 have them). Users see the error title but not how to fix it.
- **Fix**: Add help messages for all error codes.

### M7. Built-in tests for the C runtime
- **Problem**: `src/runtime/*.c` and `src/pal/linux/*.c` have no tests independent of the Mire compiler. A bug in `rt_string_equals` (used by `strcmp`) or `rt_list_push_ptr` would only show up as a Mire-level test failure.
- **Fix**: Add a C test harness that tests each runtime function in isolation.

### M8. Warning filters / deny system not tested
- **Problem**: The `WarningFilter` and `deny_warnings` options in `BuildOptions` have no dedicated tests. Warnings-as-errors behavior is not verified.
- **Fix**: Add tests that compile warning-producing code with various filter/deny settings.

---

## ⚪ Low / Stretch

### L1. Aggressive optimizations

| Optimization | Description | Difficulty | Impact |
|---|---|---|---|
| **GVN** | Global Value Numbering — eliminate redundant computations across blocks | Medium | Moderate |
| **CSE** | Common Subexpression Elimination — reuse identical subexpressions | Low | Moderate |
| **LICM** | Loop Invariant Code Motion — hoist invariant expressions out of loops | High | High (if loops are common) |
| **Constant propagation through memory** | Follow `Store` → `Load` chains of constants | Medium | Low–Moderate |
| **Tail call elimination** | Convert `ret call @f()` → `tail call @f(); ret` | Low | Moderate (recursion) |
| **MIR → SSA with φ-nodes** | Replace alloca/store/load with SSA φ, enabling more optimizations | High | High |

### L2. New error codes for uncovered diagnostics
- The compiler currently emits E0001–E0015. E0002 and E0004 are unused. Potential new codes:
  - E0016: Feature not yet implemented (codegen gap, e.g., tuples)
  - E0017: Invalid inline assembly
  - E0018: Macro expansion failure
  - E0019: Recursion limit exceeded
  - E0020: Cycle detected (in imports or type resolution)

### L3. WASM backend
- **Problem**: PAL architecture exists (`src/pal/linux/`) but only Linux is implemented. A WASM target would enable browser and edge use cases.
- **Requires**: PAL implementations for WASM, Emscripten/wasi-sdk toolchain, `src/pal/wasm/` directory.

### L4. LSP / IDE support
- **Problem**: No language server protocol implementation. Users edit `.mire` files without autocomplete, diagnostics, or jump-to-definition.
- **Requires**: A separate `mire-lsp` binary that uses the compiler's incremental analysis.

### L5. Formatter
- **Problem**: No `mire fmt` command. Code formatting is manual.
- **Requires**: A parser-based formatter, possibly using the existing AST.

---

## Completed

- ✅ MIR codegen fixes: string comparison, PAL builtins, boolean coercion, `rt_get_args` decl
- ✅ Modularization: `codegen.rs` → `codegen/`, `lower.rs` → `lower/`, `optimize.rs` → `optimize/`
- ✅ Inliner fix: `any_inlined` → `all_inlined`
- ✅ 140/140 integration tests passing, 136/136 unit tests, 0 warnings
- ✅ Error code documentation (see `ERROR_CODES.md`)
- ✅ B1 release preparation (Avenys-rust v3.11.28 + OWL v0.14.0)

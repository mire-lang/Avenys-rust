# MIR Pipeline (Mid-Level IR)

## Overview

The MIR pipeline is the **only codegen** for Avenys: a 4-phase mid-level intermediate representation lowered to LLVM IR.

Source: `src/compiler/mir/`

## Phases

### Phase 1: Types (`src/compiler/mir/mod.rs`)

Core data structures — all derive `Debug, Clone`:

| Type | Description |
|---|---|
| `MirProgram` | Top-level: functions, entry point, extern functions |
| `MirFunction` | Name, params, return type, basic blocks |
| `MirBlock` | ID, label, instruction list, terminator |
| `MirInst` | `result: Option<usize>`, `op: MirOp`, `loc` |
| `MirValue` | `Const(MirConst)`, `Temp(usize)`, `Param(String)`, `Global(String)`, `FunctionRef { name, env }`, `EnvPtr` |
| `MirConst` | `Int(i64)`, `Float(f64)`, `Bool(bool)`, `Char(char)`, `Str(String)`, `None` |

`FunctionRef { name, env }` holds the name of a mire function (user-defined, generated
closure, or extern wrapper) together with the environment pointer value. It is resolved
to `@fn_<name>` during codegen and is the first step toward a uniform first-class function
representation. `EnvPtr` represents the implicit environment pointer parameter available in
closure bodies.
| `MirOp` | Alloca, Load, Store, Add, Sub, Mul, SDiv, Shl, And, Or, ICmp, FCmp, Call, Gep, PtrToInt, IntToPtr, BitCast, ZExt, Trunc, Phi, Select, Copy |
| `MirCmp` | Eq, Ne, Lt, Le, Gt, Ge |
| `MirTerminator` | `Br(usize)`, `BrCond(MirValue, usize, usize)`, `Ret(Option<MirValue>)`, `Unreachable` |
| `MirExternFunction` | External function signatures (name, params, return type) |

### Phase 2: Lowering (`src/compiler/mir/lower.rs`)

`lower_program(program: &Program) -> MirProgram`

| AST Construct | Status |
|---|---|
| Function definitions | Done |
| Impl blocks (methods) | Done (including instance method dispatch) |
| Let/set declarations | Done |
| Assignment | Done |
| Return | Done |
| If/else | Done |
| While loops | Done |
| For loops (with optional index variable) | Done (lowered to while-loop with counter) |
| Binary ops (+, -, *, /, ==, !=, <, <=, >, >=, &&, \|\|) | Done |
| Unary ops (-, !) | Done |
| Function calls (user + builtins) | Done (builtins emit as regular calls) |
| Method calls (instance dispatch) | Done (resolves via `method_map`, prepends receiver) |
| First-class function values | Partial: `FunctionRef` symbols only; full `{fn_ptr, env_ptr}` struct value not yet materialized |
| Match expressions | Done (BrCond per case with literal/enum discriminant comparison, case blocks) |
| If-expressions (`__if_expr`) | Done (BrCond + phi-like store/load) |
| Literals (int, float, bool, char, str) | Done |
| Variable references | Done (type-aware Load via `var_types`) |
| Extern functions | Done (collected as `MirExternFunction`; wrappers generated on demand in codegen)
| Index expressions (array/map read) | Done (GEP + Load) |
| Index assignment (array/map write) | Done (GEP + Store) |
| Member access (struct field read) | Done (GEP + Load via struct metadata) |
| Member assignment (struct field write) | Done (load heap ptr + GEP + Store) |
| Struct construction via Tuple expr | Done (emitted as `Call(struct_name, args)`) |
| Reference (`&expr`) | Done (returns alloca ptr, skips Load) |
| Dereference (`*expr`) | Done (Load from pointer) |
| Unsafe blocks | Done (forwards body) |
| Closures | Partial: lowered to standalone functions; captures are always empty (parser does not emit captures yet) |
| Higher-order list functions | Done: `lists.map`, `lists.filter`, `lists.fold` lowered to loops that call the closure function |
| Pipeline (`|>`) | Not supported |
| Enum variants (bare path) | Done: resolves to real discriminant |
| Enum variants (with payload) | Partial: returns discriminant only (payloads not bound yet) |
| Try/Ok/Err | Not supported |
| Dict/Map literals | Not supported (returns `Const(None)`) |

Var types tracked via `var_types: HashMap<String, DataType>` during lowering.
Struct metadata collected by `extract_struct_types()` and passed as
`struct_types: HashMap<String, Vec<(String, DataType)>>` through `MirProgram` /
`MirLower` / `LlvmCtx`.

Enum metadata collected by `extract_enum_types()` and passed as
`enum_types: HashMap<String, Vec<(String, usize)>>` through `MirProgram` /
`MirLower` / `LlvmCtx`.

Bare-name resolution collected by `extract_bare_name_map()` and passed as
`bare_to_qualified: HashMap<String, String>` through `MirProgram` /
`MirLower` / `LlvmCtx`.

Method dispatch metadata collected by `extract_method_map()` and passed as
`method_map: HashMap<String, HashMap<String, String>>` through `MirProgram` /
`MirLower` / `LlvmCtx`.

### Phase 3: Codegen (`src/compiler/mir/codegen.rs`)

`mir_to_llvm(mir: &MirProgram) -> (String, Vec<(String, String)>)`

Returns LLVM IR text + extern libs (currently always empty).

| Feature | Status |
|---|---|
| Function defs with typed params | Done |
| Alloca for locals | Done |
| Load/Store with correct types | Done |
| Integer arithmetic (add, sub, mul, sdiv, shl) | Done |
| Float arithmetic (fadd, fsub, fmul, fdiv) | Done |
| Mixed-type arithmetic (coerce i64→double via sitofp) | Done |
| Integer comparison (icmp) | Done |
| Float comparison (fcmp) | Done |
| Boolean And/Or | Done (emits `and i1` / `or i1`) |
| Branch / conditional branch | Done |
| Return | Done |
| Function calls (typed args) | Done |
| Indirect/direct calls via `FunctionRef` | Done: direct call to `@fn_<name>` with implicit `env_ptr` argument |
| Extern function wrappers | Done: generated on demand for extern functions used as values (direct calls, `call(...)` targets, or stored in variables) |
| ZExt bool→i64 | Done |
| Trunc i64→i32 | Done |
| Extern declarations from `ExternFunction` AST | Done |
| Extern function name resolution (strip root namespace) | Done (via `split_once('.')`) |
| SSA temp type tracking | Done (`temp_types`, `param_types`, `resolve_typed`) |
| Float hex encoding | Done (`to_bits()` for exact bit representation) |
| String constants | Partial: returns `ptr null` |
| GEP (struct fields, array elements) | Done (2-index for structs, 1-index for arrays/pointers) |
| Phi, Select, PtrToInt, IntToPtr, BitCast | Done |
| Temporary ID separation: `%e{n}` for extras, `%t{mir_id}` for results | Done |
| Struct constructor calls via `Call(struct_name, ...)` | Done |
| Builtins (dasu, ireru, proc.on, etc.) | Partial: some special-case wrappers exist; `lists.map/filter/fold` are handled in MIR lowering, while other builtins still rely on kioto stubs/runtime helpers |

### Phase 4: Optimizations (`src/compiler/mir/optimize.rs`)

`optimize(program: &mut MirProgram) -> usize`

Fixed-point loop per function — iterates until no changes:

```
loop {
 constant_fold_function // const-const binops (incl. Shl)
 + algebraic_simplify // x+0→x, 0+x→x, x*1→x, 1*x→x, x*0→0, 0*x→x, x-0→x, x-x→0, x/1→x, ICmp(Eq, x,x)→true
 + strength_reduce // x*2^k → x<<k
 + copy_propagate // t1=Copy(v) → replace all Temp(t1) uses with v (transitive)
 + fold_constant_branches // BrCond(Const(true), L1, L2) → Br(L1); BrCond(Const(false), L1, L2) → Br(L2)
 + dce_function // remove unused instructions without side effects (stores/calls preserved)
 + dead_block_elim // remove blocks with 0 predecessors (entry block always kept)
 + merge_blocks // merge Br→single-predecessor chains
}
```

Returns total number of applied transformations.

#### Optimizations detail

| Optimization | Description |
|---|---|
| **Constant folding** | `Add(Const(1), Const(2))` → `Copy(Int(3))` for Add/Sub/Mul/SDiv/Shl/ICmp/FCmp/ZExt |
| **Algebraic simplification** | Identity/sink eliminations: `x+0`, `0+x`, `x*1`, `1*x`, `x*0`, `0*x`, `x-0`, `x-x`, `x/1`, `ICmp(Eq, x, x)`→true |
| **Strength reduction** | `Mul(x, Const(2^k))` → `Shl(x, Const(k))` (power-of-2 constants only) |
| **Copy propagation** | `t1 = Copy(v); t2 = Add(t1, ...)` → `t2 = Add(v, ...)`. Transitive: `t2 = Copy(t1); t3 = Add(t2, ...)` → `Add(v, ...)` |
| **BrCond folding** | `BrCond(Const(true), L1, L2)` → `Br(L1)`. Part of dead block elimination pipeline. |
| **Dead code elimination** | Removes unused instruction results; preserves `Store`, `Call` (side effects) |
| **Dead block elimination** | Removes basic blocks with 0 incoming edges after branch folding |
| **Block merging** | Merges `Br(id)` → single-predecessor successor blocks |

#### Confirmed test results (30 tests)

| Category | Tests | Status |
|---|---|---|
| Algebraic: `x+0`, `0+x`, `x*1`, `1*x`, `x*0`, `0*x`, `x-0`, `x-x`, `x/1` | 9 | Pass |
| Copy prop: simple, transitive | 2 | Pass |
| DCE: removes unused, preserves Call, preserves Store, preserves void Call, mixed | 5 | Pass |
| BrCond fold: true, false, skip-nonconst | 3 | Pass |
| Dead block: simple, keeps-entry, after-branch-folding, full-pipeline | 4 | Pass |
| Strength reduction: `x*2→x<<1`, `x*8→x<<3`, `4*x→x<<2`, non-pow2, zero, negative, pipeline | 7 | Pass |

## Integration

- **Default**: MIR pipeline is the only codegen path; there is no alternative backend.
- **Runtime helpers**: C runtime files in `src/runtime/` and `src/pal/<pal>/*.c`

## Known Limitations

1. **Builtins not fully generalized**: `dasu`, `ireru`, `len`, `proc.on`, etc. still use a mix of special-case wrappers and runtime helpers instead of a single unified builtin lowering path. `lists.map/filter/fold` are now handled in MIR lowering, but the remaining builtins still depend on kioto stubs or runtime helpers.

2. **Extern libs empty**: Second tuple element always `Vec::new()`. The existing codegen properly collects from `ExternLib` statements.

3. **String constants**: `MirConst::Str` emits `ptr null` instead of `@.str = constant [N x i8] c"..."`.

4. **First-class function values**: `FunctionRef` is only a symbol. A real `{ fn_ptr, env_ptr }` struct value has not been materialized yet, so closures cannot capture variables and function values cannot be stored in variables/structs/returned from functions. All indirect calls currently pass `ptr null` as the environment pointer.

5. **Inline closure expansion is a transitional path**: `call((x) => ..., ...)` still has a dedicated inline-expansion path in the lowerer. Once function values are real structs and `call()` works with any function-typed value, this path should be removed.

6. **Dict/Map literals, enums with payloads**: Dict/map literals return `Const(None)` in some paths. Enum variants with payloads return discriminant only (payload data not bound/marshalled). Enum payload bindings in match patterns are not extracted.

7. **Memory pressure**: Full `build` (compile + link via clang) can use ~4GB RAM — a pre-existing issue from clang runtime compilation, not specific to MIR.

## Calling Convention

Every mire function (user-defined, generated closure, or extern wrapper) has an
implicit first parameter:

```llvm
define i64 @fn_name(ptr %env_ptr, i64 %arg_0, ...) { ... }
```

- `env_ptr` is currently always `ptr null` for direct calls and non-capturing
 closures.
- `extern fn` declarations referenced as function values get a generated LLVM
 wrapper `@fn_<name>_wrapper(ptr %env_ptr, ...)` that forwards to the real C
 symbol. Wrappers are generated on demand to avoid bloating the module for
 externs that are only called directly.
- `FunctionRef` stores the environment value alongside the function name, and
 `EnvPtr` allows closure bodies to read the implicit environment pointer.
- The `call(...)` builtin in MIR codegen resolves the callee as a
 `FunctionRef`, bitcasts the function pointer to the call-site signature, and
 passes `ptr null` as the first argument.

## Architecture Notes

- SSA-like IR with basic blocks and explicit terminators
- Type tracking per-temp (`temp_types` HashMap in `LlvmCtx`)
- Param types per-function (`param_types`)
- Lowering is single-pass (AST has structured control flow — no CFG construction needed)
- Codegen produces standard LLVM textual IR (compatible with clang/llc)
- Optimizations are module-level transforms on `MirProgram` before codegen (no LLVM opt passes)

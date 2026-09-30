# Mire ABI v4

> Stable Application Binary Interface for the Mire programming language.

Version: **4.1.0**
Status: **Stable**
Date: **2026-09-12**

---

## Overview

The **Mire ABI v4** is the stable Application Binary Interface for the Mire programming language. It defines how validated Mire programs are represented in machine code.

### What is the Mire ABI

- **Memory layout** - size, alignment, padding, field offsets for all types
- **Calling convention** - argument passing, return values, register usage
- **Symbol naming and versioning** - mangling scheme, visibility, ELF versioning
- **FFI boundary** - C interoperability rules

### What is NOT the Mire ABI

The following are **language semantics**, not ABI concerns:

- Ownership metadata (no `S`/`R`/`M` suffixes in symbols)
- Lifetime encoding (no lifetime parameters in symbols)
- Borrow metadata (no borrow info in binary)
- Generic type representation (monomorphized internally)
- Async state machine layout
- Panic/unwind mechanics (v1 = abort)
- PAL internals (separate PAL ABI v4)

---

## C ABI Compatibility

Mire provides **full C ABI compatibility** at the FFI boundary. When you declare:

```mire
extern fn printf: (fmt :*const i8, ...) :i32 lib "c"
```

The call uses the **target platform's C calling convention** (System V AMD64, AArch64, Windows x64). No Mire ABI conventions apply at this boundary.

### Key Points

- `extern fn ... lib "c"` emits LLVM `declare` with C calling convention
- Scalar types map to exact-width C counterparts (`i64` <-> `int64_t`, `f64` <-> `double`, `bool` <-> `int`)
- Strings passed as `char*` (NUL-terminated) or `{ ptr: *const u8, len: usize }` for sized strings
- Structs passed by value (registers/sret per C ABI) or by pointer
- Callbacks C -> Mire and Mire -> C supported via function pointers and environment

---

## Type Layout

### Primitive Types

| Type | Size | Alignment |
|------|------|-----------|
| `i8` / `u8` | 1 | 1 |
| `i16` / `u16` | 2 | 2 |
| `i32` / `u32` | 4 | 4 |
| `i64` / `u64` | 8 | 8 |
| `i128` / `u128` | 16 | 16 |
| `f32` | 4 | 4 |
| `f64` | 8 | 8 |
| `bool` | 1 | 1 |
| `char` | 4 | 4 |
| `ptr` / `*const T` / `*mut T` | 8 | 8 |

### Composite Types

- **Structs**: C-style field order, standard padding, trailing padding to alignment multiple
- **Enums**: Discriminant (smallest integer fitting all variants) + payload union
- **Arrays**: `[N x T]` - contiguous, no metadata
- **Str**: `{ ptr: *const u8, len: usize }` - fat pointer
- **Slices**: `{ ptr: *const T, len: usize }` - fat pointer
- **References**: `&T` = thin pointer for sized T, fat pointer for unsized

---

## Calling Convention (x86_64 System V)

### Argument Passing

```
fn_name(env_ptr, arg0, arg1, arg2, arg3, arg4, arg5, ...)
        RDI      RSI   RDX   RCX   R8    R9    stack...
```

- **Implicit `env_ptr`** (first arg): Always present, currently `null` for non-capturing functions
- **Registers**: RDI, RSI, RDX, RCX, R8, R9 (INTEGER class), XMM0-XMM7 (SSE class)
- **Stack**: Remaining arguments, 16-byte aligned
- **Aggregates > 16 bytes**: Passed via hidden sret pointer in RDI

### Return Convention

| Size | Convention |
|------|------------|
| <= 8 bytes | RAX |
| 9-16 bytes | RAX + RDX |
| > 16 bytes | Caller-allocated sret pointer in RDI, return in RAX |

---

## Symbol Naming

```
Function:  _M<module_len><module><func_len><func><types>
Example:   _M5Math3addI64I64I64

Method:    _M<module_len><module><type_len><type><method_len><method><types>
Closure:   _C<capture_hash><func_len><func><types>
```

### Type Encodings

| Code | Meaning |
|------|---------|
| I8, I16, I32, I64, U8, U16, U32, U64 | Integers |
| F32, F64 | Floats |
| B | Bool |
| P | Pointer |
| S | Str (fat pointer) |
| V | Vec |
| M | Map |
| R | Ref (&T) |
| Q | Mut Ref (&mut T) |
| X | Box |
| T<types> | Tuple |
| A<count><type> | Array |
| Z | Zero-sized |

**No lifetimes, no ownership markers, no borrowing metadata in symbols.**

---

## Symbol Versioning

- Uses **ELF symbol versioning** (`SHT_GNU_verdef` / `SHT_GNU_verneed`)
- **No `@v1` in mangling** - versioning is separate from mangling
- Compatible changes: new symbols, new optional parameters
- Breaking changes: removed symbols, changed signatures, struct layout changes

---

## FFI Layer (C Interoperability)

| Aspect | Mire ABI | C ABI (via FFI) |
|--------|----------|-----------------|
| Calling convention | Custom (implicit env_ptr) | Target C ABI |
| Symbol naming | Mangled (`_M...`) | Unmangled / `#[no_mangle]` |
| Type layout | Specified in this doc | C standard layout |
| Return convention | Registers/sret per this ABI | Per C ABI |

### Attributes

- `#[export_c]` - emits C symbol, C calling convention
- `extern "C" { ... }` - C calling convention, no env_ptr
- `#[no_mangle]` - disable mangling

---

## Runtime Tiers & ABI

| Tier | Runtime Symbols | PAL Symbols |
|------|-----------------|-------------|
| `full` | All `rt_*` available | All `pal_*` available |
| `minimal` | Demand-driven (used only) | Demand-driven |
| `none` | None (user provides) | User provides |

---

## Object and Library Production

Mire IR is lowered to native objects by LLVM tooling. Static Mire libraries use `llc` for IR-to-object lowering and `llvm-ar` for archive creation. PAL and runtime C sources remain separate inputs: they are compiled as C objects and linked into the final library using their documented C ABI.

Shared-library linking uses `ld.lld` directly. Executable linking uses the Clang driver only as a target-aware CRT/libc/native-library link stage; Mire IR has already been lowered to an object by `llc`.

---

## ABI Conformance Rule

Documentation is not sufficient to establish ABI compatibility. A release is ABI-conformant only when the compiler's emitted symbols, layouts and calling conventions pass the ABI symbol and runtime conformance suites for the target. POO constructs (structs, impl methods, skills and associated methods) use the same Mire symbol and layout rules as ordinary functions; they do not introduce a second object model or a hidden runtime dependency.

---

## Verification

```bash
cargo test --release --test abi_consistency
cargo test --release --test pal_conformance
# Expected: 126 symbols (128 compiler-emitted minus 2 struct-return)
```

---

## Related

- [Runtime Config](../rt/README.md)
- [PAL v4](../pal/README.md)
- [Avenys Doc Index](../README.md)

---

## License

GNU General Public License v3.0

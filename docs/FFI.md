# Foreign Function Interface (FFI)

## Overview

Mire supports calling C functions from dynamic libraries (.so, .dylib)
and the standard C runtime. Functions are declared with `extern fn`
and libraries with `extern lib`.

## Syntax

```mire
extern lib "SDL2"
extern lib "mylib" "/usr/lib/libmylib.so"

extern fn function_name: (params) :return_type lib "library"
```

**Single-token** `extern lib "name"` uses the name as both identifier
and link flag (`-lname`).

**Two-token** `extern lib "name" "/path/to/lib.so"` adds `-L/path`
and `-lname` to the linker.

## Supported Types

| Mire type | C type | Notes |
|-----------|--------|-------|
| `i8` | `int8_t` | 8-bit signed integer |
| `i16` | `int16_t` | 16-bit signed integer |
| `i32` | `int32_t` | 32-bit signed integer |
| `i64` | `int64_t` / `long` | 64-bit signed integer |
| `i128` | `__int128` | 128-bit signed integer |
| `u8` | `uint8_t` | 8-bit unsigned integer |
| `u16` | `uint16_t` | 16-bit unsigned integer |
| `u32` | `uint32_t` | 32-bit unsigned integer |
| `u64` | `uint64_t` | 64-bit unsigned integer |
| `u128` | `unsigned __int128` | 128-bit unsigned integer |
| `f32` | `float` | 32-bit IEEE float |
| `f64` | `double` | 64-bit IEEE float |
| `bool` | `int` (0/1) | Boolean |
| `char` | `uint32_t` | UTF-32 code point |
| `str` | `char*` | Null-terminated string |
| `*mut i8` | `void*` / `char*` | Raw mutable pointer |
| `*const i8` | `const void*` | Raw const pointer |

## Examples

### Calling libc

```mire
extern fn puts: (msg :*mut i8) :i32 lib "c"

pub fn main: () {
 puts("Hello from libc!")
}
```

### SDL2 bindings

```mire
module sdl2

extern lib "SDL2"

extern fn SDL_Init: (flags :i64) :i64 lib "SDL2"
extern fn SDL_Quit: () lib "SDL2"

pub fn init_video: () :bool {
 return SDL_Init(0x00000020) == 0
}

pub fn quit: () {
 SDL_Quit()
}
```

## How it works

1. **Parser**: `extern fn ... lib "name"` → `Statement::ExternFunction`
 with `lib_name` field. `extern lib "name" "path"` → `Statement::ExternLib`.

2. **MIR lowerer**: `ExternFunction` → `MirExternFunction { lib_name, ... }`.
 `ExternLib` → added to `MirProgram.extern_libs`.

3. **Codegen**: `MirExternFunction` → LLVM `declare` statement.
 `extern_libs` returned alongside the IR.

4. **Linking**: `toolchain.rs` iterates `extern_libs`, generating
 `-L/path` for .so parent directories and `-lname` for the linker.

5. **Runtime**: Dynamic linker resolves symbols at program start.

## Limitations

 - No struct passing by value between Mire and C
 - No callback support (C calling Mire functions)
 - No Rust interop (separate effort needed)
 - Scalar types map to their exact-width C counterparts (see type table above)
- `extern lib` names get module-prefixed (stripped at link time)

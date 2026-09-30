// Foreign Function Interface (FFI)

## Overview

Mire supports calling C functions from dynamic libraries and the standard C runtime.

## Declaration

```mire
// Library declaration
extern lib "SDL2"
extern lib "mylib" "/usr/lib/libmylib.so"

// Function declaration
extern fn function_name: (params) :return_type lib "library"
```

- `extern lib "name"` - single token, uses `-lname`
- `extern lib "name" "/path"` - two tokens, adds `-L/path -lname`
- `extern fn ... lib "library"` - declares external function

## Type Mapping

| Mire Type | C Type | Notes |
|-----------|--------|-------|
| `i8` | `int8_t` | 8-bit signed |
| `i16` | `int16_t` | 16-bit signed |
| `i32` | `int32_t` | 32-bit signed |
| `i64` | `int64_t` / `long` | 64-bit signed |
| `i128` | `__int128` | 128-bit signed |
| `u8` | `uint8_t` | 8-bit unsigned |
| `u16` | `uint16_t` | 16-bit unsigned |
| `u32` | `uint32_t` | 32-bit unsigned |
| `u64` | `uint64_t` | 64-bit unsigned |
| `u128` | `unsigned __int128` | 128-bit unsigned |
| `f32` | `float` | IEEE 32-bit |
| `f64` | `double` | IEEE 64-bit |
| `bool` | `int` (0/1) | Boolean |
| `char` | `uint32_t` | UTF-32 codepoint |
| `str` | `char*` | NUL-terminated |
| `*mut i8` | `void*` / `char*` | Raw mutable pointer |
| `*const i8` | `const void*` | Raw const pointer |

## Examples

##// libc

```mire
extern fn puts: (msg :*mut i8) :i32 lib "c"

pub fn main: () {
    puts("Hello from libc!")
}
```

##// SDL2

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

## Binding to PAL (kioto Pattern)

```mire
// kioto/core/fs/mod.mire
extern fn pal_root_remove: (root :i64, path :&str) :bool lib "c"
extern fn pal_last_error: () :i64 lib "c"

pub fn remove: (path :&str) :bool {
    set root = pal_root_open(fs::path::dir(path))
    if root <= 0 { return false }
    set ok = pal_root_remove(root, fs::path::name(path))
    pal_root_close(root)
    return ok
}
```

- Declare exactly needed PAL primitives
- Wrap in safe Mire functions
- Compose recursion in Mire, not PAL

## How It Works

1. **Parser**: `extern fn` -> `Statement::ExternFunction` with `lib_name`
2. **MIR Lowering**: `ExternFunction` -> `MirExternFunction`
3. **Codegen**: `MirExternFunction` -> LLVM `declare`
4. **Linking**: `toolchain.rs` adds `-L/path -lname` for linker

## Limitations

1. **No struct passing by value** between Mire and C
2. **No callbacks** (C calling Mire functions)
3. **No Rust interop** (separate effort needed)
4. **Scalar types only** - exact-width C counterparts
5. **`extern lib` names module-prefixed** (stripped at link time)

## C ABI vs Mire ABI

| Aspect | Mire ABI | C ABI (via FFI) |
|--------|----------|-----------------|
| Calling convention | Custom (env_ptr first) | Target C ABI |
| Symbol naming | Mangled (`_M...`) | Unmangled / `#[no_mangle]` |
| Type layout | Specified in ABI doc | C standard layout |
| Return convention | Registers/sret per ABI | Per C ABI |

**Key**: `extern fn ... lib "c"` uses **C ABI** at boundary. Mire ABI used internally.

## Exporting Mire Functions to C

```mire
// In library (libt = "cdylib")
@[export_c]
pub fn add: (a :i64, b :i64) :i64 {
    return a + b
}
```

- `#[export_c]` emits C symbol with C calling convention
- No implicit `env_ptr` parameter
- Symbol not mangled

## Attributes

| Attribute | Effect |
|-----------|--------|
| `#[export_c]` | C symbol, C calling convention |
| `extern "C" { ... }` | C calling convention block |
| `#[no_mangle]` | Disable symbol mangling |

## Safety

- FFI is **unsafe** by nature
- Mire cannot verify C signatures
- Wrong declarations -> undefined behavior
- Wrap in safe Mire functions (kioto pattern)
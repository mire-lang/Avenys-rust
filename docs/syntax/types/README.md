# Types

## Primitive Types (Fixed Width)

| Category | Types | Default |
|----------|-------|---------|
| Signed integers | `i8`, `i16`, `i32`, `i64`, `i128` | `i64` |
| Unsigned integers | `u8`, `u16`, `u32`, `u64`, `u128` | `u64` |
| Floats (IEEE 754) | `f32`, `f64` | `f64` |
| Boolean | `bool` | - |
| Character | `char` (UTF-32) | - |
| String | `str` | - |
| Unit/Void | (unit value only in fn returns) | - |

## Type Ascription

```mire
set x = 42 :i64        // Explicit type
set y = 3.14 :f64      // Float requires explicit or inference
set z = true :bool
set c = 'A' :char
set s = "hello" :str
set u = ()  // Unit value (only in fn returns)
```

- `:Type` after value specifies exact type
- Required when inference is ambiguous or for non-default widths
- Prevents silent truncation/widening

## Conversion Rules

## Integer Widening (Safe, Implicit)

```mire
set a = 100 :i16
set b = a :i32         // OK: i16 -> i32 (zero/sign extend)
set c = b :i64         // OK: i32 -> i64
```

## Integer Narrowing (Explicit Required)

```mire
set a = 1000 :i64
// set b = a            // Error: implicit narrowing forbidden
set b = a :i16         // OK: explicit ascription
```

## Integer <-> Float (Explicit Required)

```mire
set i = 42 :i64
set f = i :f64         // OK: explicit i64 -> f64
set i2 = f :i64        // OK: explicit f64 -> i64 (truncates)
```

## Boolean

```mire
set b = true :bool
// set i = b            // Error: no implicit bool -> int
set i = (if b { 1 } else { 0 }) :i64
```

## Composite Types

## Strings (`str`)

```mire
set s = "hello" :str
// Internally: fat pointer { ptr: *u8, len: usize }
```

- UTF-8 encoded, length-prefixed
- Immutable (use `str::concat` etc. for new strings)

## Arrays (`arr[T N]`)

```mire
set arr = [1 2 3 4 5] :arr[i64 5]
// Fixed size, stack-allocated, compile-time length
set x = arr at 2       // Index access
```

## Vectors (`vec[T]`)

```mire
set v = [] :vec[i64] mut
vec::push(v, 1)
vec::push(v, 2)
// Dynamic, heap-allocated
```

## Maps (`map[K V]`)

```mire
set m = {} :map[str i64] mut
map::set(m, "key", 42)
// Hash map, heap-allocated
```

## References

```mire
set x = 10 :i64 mut
set ref = &x           // &i64 (borrowed reference)
set mut_ref = &mut x   // &mut i64 (mutable borrow)
```

- `&T` = shared borrow (read-only)
- `&mut T` = exclusive borrow (read-write)
- Borrow checker enforces: no mutable + shared simultaneously

### Raw Pointers (FFI)

```mire
set p :*mut i8 = ptr_null()
set cp :*const i8 = ptr_null()
```

- Only for FFI interop
- No safety guarantees

## Zero-Sized Type

```mire
set u = () :mu
// Unit type, zero bytes, used for functions returning nothing
```

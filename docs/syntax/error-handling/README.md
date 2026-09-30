// Error Handling

## Result Type

```mire
enum Result[T E] {
    Ok(value :T)
    Err(error :E)
}
```

- `Result[T E]` - success with `T`, failure with `E`
- `Result[T]` defaults `E = str`

## Constructors

```mire
set success = Result::Ok(42)
set failure = Result::Err("not found")
set failure = Result::Err(ErrCode::NotFound)
```

## Methods

```mire
// Unwrapping
res.unwrap()              // Returns T or panics
res.unwrap_or(0)          // Returns T or default
res.unwrap_or_else((e) => default)  // Default via closure

// Transformation
res.map((x) => x * 2)     // Transforms Ok value
res.map_err((e) => e)     // Transforms Err value

// Query
res.is_ok()               // bool
res.is_err()              // bool
```

## Try Operator (?)

```mire
pub fn read_config: () :Result[Config str] {
    set content = fs::read("config.toml")?  // Propagates Err
    set parsed = parse_toml(content)?       // Propagates Err
    return Result::Ok(parsed)
}
```

- `expr?` unwraps `Ok` or returns `Err` from function
- Function must return `Result` (or compatible)
- Only works in functions returning `Result`

## Match on Result

```mire
match read_config() {
    Result::Ok(cfg) => use(cfg)
    Result::Err(err) => dasu("Config error: {err}")
}
```

## Option / Maybe Type

```mire
enum Maybe[T] {
    None
    Some(value :T)
}
```

- Unboxed representation: `{ i1 tag, T value }`
- Zero runtime overhead

##// Constructors

```mire
set some = Maybe::Some(42)
set none = Maybe::None
```

##// Methods

```mire
opt.unwrap()              // Returns T or panics
opt.unwrap_or(0)          // Returns T or default
opt.is_some()             // bool
opt.is_none()             // bool
opt.map((x) => x * 2)     // Transforms if Some

// Try operator
opt?                      // Unwraps Some or returns None
```

## Custom Error Types

```mire
enum MyError {
    NotFound(path :str)
    PermissionDenied
    InvalidFormat(msg :str)
}

pub fn open_file: (path :str) :Result[File MyError] {
    if !fs::exists(path) {
        return Result::Err(MyError::NotFound(path))
    }
    // ...
}
```

## Panic

```mire
// Explicit panic
panic("Something went wrong")

// Assertion
assert(condition, "Message if false")

// Unreachable
unreachable("This code should never execute")
```

- `panic(msg)` calls `rt_panic_loc` with message
- `assert` only in debug builds (unless `--deny-warnings`)
- `unreachable` for compiler-known impossible paths

## Error Codes

Mire uses structured error codes:

| Code | Category | Description |
|------|----------|-------------|
| E0001 | Parse | Syntax error |
| E0002 | Type | Type mismatch |
| E0003 | Borrow | Use after move |
| E0004 | Borrow | Mutable + shared borrow |
| E0005 | Borrow | Not mutable |
| E0015 | Runtime | General runtime error |
| E0025 | Module | `use!` with `load` |
| E0026 | Module | `load!` without `use!` |
| W0001 | Warning | Unused variable |
| W0002 | Warning | Unused function |
| ... | ... | ... |

See [ERROR_CODES.md](../ERROR_CODES.md) for full list.

## Best Practices

1. **Use `Result` for recoverable errors** - file I/O, parsing, network
2. **Use `Maybe` for optional values** - lookup, search, parsing
3. **Use `panic` for unrecoverable bugs** - violated invariants
4. **Propagate with `?`** - clean error flow
5. **Match exhaustively** - handle all cases
6. **Custom error enums** - for domain-specific errors
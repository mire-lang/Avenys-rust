// Enums

## Declaration

```mire
// Simple variants
pub enum Color {
    Red
    Green
    Blue
}

// Variants with payload
pub enum Option[T] {
    None
    Some(value :T)
}

pub enum Result[T E] {
    Ok(value :T)
    Err(error :E)
}

// Multiple payloads
pub enum Event {
    KeyPress(key :char)
    MouseMove(x :i64, y :i64)
    Resize(width :i64, height :i64)
    Quit
}
```

- `pub enum` = exported
- Variants listed one per line
- Payload types in parentheses
- Generic parameters: `enum Name[T] { ... }`

## Construction

```mire
// Simple variant
set c = Color.Red

// Payload variant
set opt = Option::Some(42)
set err = Result::Err("not found")

// With type ascription
set opt = Option::Some(42) :Option[i64]
```

- `Enum.Variant` for simple
- `Enum.Variant(payload)` for payload
- Generic instantiation: `Enum[T]::Variant(...)`

## Pattern Matching

```mire
match opt {
    Option::None => "empty"
    Option::Some(v) => "value: {v}"
}

match result {
    Result::Ok(v) => dasu("Success: {v}")
    Result::Err(e) => dasu("Error: {e}")
}

match event {
    Event::KeyPress(k) => dasu("Key: {k}")
    Event::MouseMove(x, y) => dasu("Mouse at {x},{y}")
    Event::Quit => exit()
}
```

- `match` expression/statement
- Payload extracted via binding: `Some(v)`
- Multiple payloads: `Variant(x, y)`
- Exhaustiveness checked (error if missing variant)

## Methods on Enums

```mire
impl Option[T] {
    pub fn is_some: (self) :bool {
        match self {
            Some(_) => true
            None => false
        }
    }
    
    pub fn unwrap: (self) :T {
        match self {
            Some(v) => v
            None => panic("unwrap on None")
        }
    }
}

set opt = Option::Some(42)
if opt.is_some() {
    dasu(opt.unwrap())
}
```

- Inside `impl Enum[T] { ... }`
- `self` matches on enum variants
- Can return payload types

## Result and Option Helpers

```mire
// Option
set opt = Option::Some(42)
opt.unwrap()              // Returns value or panics
opt.unwrap_or(0)          // Returns value or default
opt.map((x) => x * 2)     // Transforms if Some

// Result
set res = Result::Ok(42)
res.unwrap()              // Returns Ok value or panics
res.unwrap_or(0)          // Returns Ok value or default
res.map_err((e) => e)     // Transforms Err
```

- Standard methods on `Option` and `Result`
- `?` operator for propagation (see error-handling)

## Visibility

```mire
pub enum PublicEnum {  // Exported
    pub Variant1       // Exported variant
    Variant2           // Module-internal variant
}

enum PrivateEnum {     // Module-internal
    ...
}
```

- `pub enum` = exported
- `pub Variant` = exported variant
- No `pub` = module-private

## Layout

```
Enum representation:
- Discriminant: smallest integer fitting all variants (i8/i16/i32)
- Payload: union of all variant payloads
- Total size = discriminant + max payload size
- Alignment = max(payload alignments)
```

## Limitations

1. **No associated data on simple variants** - use payload variants
2. **No method overriding** - all methods in single impl block
3. **No default variant** - all must be handled in match
4. **Recursive enums** - use `Box[T]` for self-referential payloads
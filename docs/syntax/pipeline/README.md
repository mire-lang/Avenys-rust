// Pipeline Operators

## Standard Pipeline (=>)

```mire
set result = 5
    => add(3)
    => mul(2)
    => str::from_i64
// result = "16"
```

- `=>` passes left value as **first argument** to right function
- Chains multiple operations
- Reads left-to-right (like Unix pipes)

## Safe Pipeline (?=>)

```mire
set result = Maybe::some(10)
    ?=> (x) => if x > 5 { Maybe::some(x * 2) } else { Maybe::none }
    ?=> (x) => Maybe::some(x + 1)
// If any step returns None, pipeline stops and returns None
```

- `?=>` propagates `Maybe`/`Result` errors
- If left side is `None`/`Err`, stops and returns that
- Only passes `Some`/`Ok` value to next function

## Self Marker (Position Control)

```mire
// Default: value passed as FIRST argument
set a = "hello" => str::concat("world")  // concat("hello" "world")

// Explicit self marker for other positions
set b = "hello" => str::concat(self "world")  // Same as above
set c = "world" => str::concat("hello" self)  // concat("hello" "world")

// Multiple self markers (if function takes multiple)
set d = 10 => math::add(self 5)  // add(10 5)
set e = 5 => math::add(10 self)  // add(10 5)
```

- `self` keyword marks where piped value goes
- Default: first position (if no `self` present)
- Allows piping to any parameter position

## Practical Examples

```mire
// String processing pipeline
set cleaned = input
    => str::trim
    => str::lower
    => str::replace("  " " ")
    => str::split(" ")

// Numeric pipeline
set result = 100
    => math::sqrt
    => (x) => x * 2
    => str::from_f64

// Error handling pipeline
set config = read_file("config.toml")
    ?=> parse_toml
    ?=> validate_config
    ?=> (c) => Maybe::some(c)
    => (c) => apply_config(c)
```

## Precedence

Pipeline operators have **lowest precedence**:

```mire
set x = a + b => f  // Parsed as: (a + b) => f
set y = a => f + b  // Parsed as: a => (f + b)  [if f returns numeric]
```

- Use parentheses for clarity
- `=>` and `?=>` bind looser than all other operators

## Limitations

1. **First-argument only by default** - use `self` for other positions
2. **No implicit tuple unpacking** - `=>` passes single value
3. **Type inference** - may need explicit types in complex chains
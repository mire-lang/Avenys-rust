// Closures

## Syntax

```mire
// Single expression
set add_one = (x :i64) => x + 1

// Multiple parameters
set add = (a :i64, b :i64) => a + b

// Block body (multiple statements)
set process = (x :i64) => {
    set y = x * 2
    return y + 1
}

// No parameters
set get_five = () => 5
```

- `=>` arrow syntax
- Parentheses required for parameters
- Block body with `{ ... }` for multiple statements
- Last expression in block is return value

## Environment Capture

```mire
set multiplier = 10 :i64

set scale = (x :i64) => x * multiplier  // Captures 'multiplier'

set m = 20
// scale still uses original captured value (10)
```

- Captures variables from enclosing scope **by value** (move)
- Captured variables become part of closure's environment
- No explicit capture syntax needed

## First-Class Usage

```mire
set nums = [1 2 3 4 5] :vec[i64]

// Map with closure
set doubled = vec::map::i64_i64(nums, (x) => x * 2)

// Filter with closure
set evens = vec::filter::i64(nums, (x) => x % 2 == 0)

// Fold with closure
set sum = vec::fold::i64(nums, 0, (acc x) => acc + x)
```

- Passed as function values to higher-order functions
- Type: `fn(param_types) :return_type`
- Can be stored in variables, passed to functions, returned

## Higher-Order Functions

```mire
// vec::map::i64_i64: (v :&vec[i64], f fn(i64) :i64) :vec[i64]
// vec::filter::i64: (v :&vec[i64], f fn(i64) :bool) :vec[i64]
// vec::fold::i64: (v :&vec[i64], init :i64, f fn(i64 i64) :i64) :i64
```

- Standard library provides `map`, `filter`, `fold` for collections
- Also `find`, `partition`, `chunk`, `window`

## Type Annotations

```mire
// Explicit closure type
set f: fn(i64) :i64 = (x) => x * 2

// In function signature
pub fn apply: (f fn(i64) :i64, x :i64) :i64 {
    return f(x)
}
```

- Closure type: `fn(param_types) :return_type`
- Used in variable declarations, parameters, return types

## Limitations (Current)

1. **No mutable captures** - captured variables are moved, not borrowed
2. **No recursive closures** - closure cannot call itself
3. **Capture by value only** - no `&` or `&mut` in captures
4. **No async closures** - use `async::task::spawn` for async work
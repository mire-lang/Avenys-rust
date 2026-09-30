// Macros

## Overview

Mire macros are **syntactic sugar over function calls** with special invocation syntax.

## Declaration

```mire
@[macro!]
pub fn assert: (cond :bool, msg :str) {
    if !cond {
        panic("Assertion failed: {msg}")
    }
}

@[macro!]
pub fn dbg: (value :T) :T {
    dasu("DEBUG: {value}")
    return value
}
```

- `@[macro!]` attribute on function
- Function must be `pub`
- Can be generic: `pub fn dbg[T]: (v :T) :T`

## Invocation

```mire
// Function-style (with parens)
assert!(x > 0, "x must be positive")

// Statement-style (no parens for unit return)
assert!(condition)

// With type arguments
dbg![i64](42)
```

- `name!(args)` syntax
- Arguments parsed as expressions
- Type args in square brackets: `name![T](args)`

## Owl.toml Whitelist

```toml
[security]
macros = ["assert", "dbg", "panic", "unreachable"]
```

- Only listed macros can be invoked with `!`
- E0019: macro not in whitelist
- E0020: macro not declared with `@[macro!]`

## Built-in Macros

| Macro | Description |
|-------|-------------|
| `assert!(cond, msg?)` | Panic if false |
| `dbg!(expr)` | Print debug, return value |
| `panic!(msg)` | Unconditional panic |
| `unreachable!(msg?)` | Mark unreachable code |

## How It Works

1. **Parser**: `name!(args)` -> `Expression::Macro`
2. **Type Check**: Validates macro exists, whitelisted, args match
3. **MIR Lowering**: Translates to `Call` to macro function
4. **Codegen**: Regular function call

## Design Properties

##// Runtime Execution Only

- **No compile-time expansion** - macro runs at runtime
- **No AST manipulation** - no macro hygiene issues
- **No host code execution** - pure Mire

##// Hygiene Guaranteed

- Macros are regular functions with lexical scope
- No token capture, no name collision
- Parameters are explicit

##// Zero Compile-Time Cost

- No macro expansion phase
- No additional compilation passes
- Regular function call overhead only

## Whitelist Validation

```mire
// E0019: Macro 'custom' not in owl.toml [security] macros list
custom!(42)

// E0020: Function 'foo' not declared with @[macro!]
foo!(42)
```

- Compile-time validation via `owl.toml`
- Strict mode enforces strictly

## Limitations

1. **No variadic arguments** - fixed arity
2. **No code generation** - cannot create new syntax
3. **No compile-time computation** - runtime only
4. **No attribute macros** - only function-like
5. **No derive macros** - use `@[derive(...)]` instead
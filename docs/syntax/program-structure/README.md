// Program Structure

## Entry Point

Every Mire program must have a `main` function:

```mire
pub fn main: () {
    // Program code here
}
```

- `pub` makes it visible as entry point
- `fn main: ()` takes no parameters, returns unit (`mu`)
- Return type `()` can be omitted for unit

## Comments

```mire
// Line comment

/! Block comment
   can span multiple lines
!/

/! Nested /! block comments !/ are supported !/
```

**Migration Note**: Since v3.11.38, `#` and `//!` are no longer valid comment syntax. Use `//` and `/! !/` only.

## Metadata Attributes

Attributes annotate declarations and are placed before them:

```mire
@[test]
pub fn test_add: () :bool { ... }

@[test][section("math")]
pub fn test_mul: () :bool { ... }

@[deprecated("use new_fn instead")]
pub fn old_fn: () { ... }

@[allow(dead_code)]
pub fn helper: () { ... }

@[deny(unsafe)]
pub fn safe_only: () { ... }
```

- **Simple**: `@[test]`
- **Chained**: `@[test][section("math")]`
- **Comma-separated**: `@[test, section("math")]`
- **With arguments**: `@[deprecated("message")]`
- **Contiguous lines** accumulate on the following declaration

## Modules

```mire
module my_module

// Declarations here belong to my_module
pub fn foo: () { ... }
```

- `module name` declares a module namespace
- All following declarations until next `module` or EOF belong to it
- Modules can be nested via `load` (see modules documentation)

## Source File Organization

```
project/
├── owl.toml              // Package manifest
├── code/
│   └── main.mire         // Entry point (default)
├── core/
│   └── utils.mire        // Internal modules
└── tests/
    └── test_main.mire    // Test files
```

- `owl.toml [project] entry` specifies entry file (default: `code/main.mire`)
- Modules loaded via `load` from dependencies or `load!` for local files
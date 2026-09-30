// Modules

## Package Modules (load)

```mire
// From owl.toml dependencies
load mire::str
load kioto::fs
load mylib::math

// Direct calls (no use! needed)
set s = str::from_i64(42)
set content = fs::read("file.txt")
```

- `load package::module` loads from dependency
- Call directly: `module::function()`
- **Error E0025** if using `use!` with `load`

## Local Modules (load!)

```mire
// Relative to current file
load! ./utils
load! ../common/helpers

// Usage requires use!
set result = use! utils::helper(42)
set result = use! helpers::process(data)
```

- `load! path` loads local `.mire` file
- **Must use `use!`** to call: `use! module::function()`
- **Error E0026** if calling without `use!`
- Path relative to file containing `load!`
- Max depth: 2 levels under project root

## Exports

```mire
// In loaded module
module mymodule

pub fn public_fn: () { ... }
fn private_fn: () { ... }  // Not accessible externally
```

- `pub` on functions = exported
- No `pub` = module-private
- Structs/enums/skills: `pub` on type = exported

## owl.toml Exports

```toml
[exports]
mymodule = "core/mymodule"
strings = "core/strings"
```

- Maps public name to module path
- Consumer does: `load mylib::mymodule`

## Dependency Declaration

```toml
[dependencies]
mire = { path = "~/.owl/libs/mire" }
kioto = { path = "~/.owl/libs/kioto", version = "2.4.8" }
mylib = { path = "../my-lib" }
```

- `path` = local path or `~/.owl/libs/...`
- `version` optional (for registry)
- Transitive deps loaded automatically

## Import Resolution Order

1. **Manifest imports** (`owl.toml [imports]`)
2. **Project local** (`load! ./...`)
3. **Owl home** (`~/.owl/modules/...`)
4. **Bundled stdlib** (if any)

## Module Namespaces

```mire
// Package module
load mire::vec
vec::push(v, 1)

// Nested module
load kioto::crypto::hash
hash::sha256(data)

// Re-export
load mire::str
pub use mire::str::{from_i64, to_i64}
```

- `package::module::function` call syntax
- `pub use` re-exports symbols

## Cyclic Dependencies

- Not allowed between packages
- Allowed within same package (mutual `load!`)
- Loader detects and reports cycles

## Visibility Summary

| Declaration | Exported? |
|-------------|-----------|
| `pub fn` | Yes |
| `fn` | No |
| `pub struct` | Yes |
| `struct` | No |
| `pub enum` | Yes |
| `enum` | No |
| `pub skill` | Yes |
| `skill` | No |
| `pub module` | Yes |
| `module` | No |

## Best Practices

1. **Use `load` for dependencies** - direct calls
2. **Use `load!` for local files** - requires `use!`
3. **Keep modules small** - single responsibility
4. **Use `pub use` for clean APIs** - hide internal structure
5. **Avoid deep nesting** - max 2 levels for `load!`
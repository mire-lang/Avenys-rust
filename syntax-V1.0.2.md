# Mire Syntax Reference

This document describes the currently frozen surface syntax of Mire as supported by the parser and the compiler toolchain today.

It is intended as the practical reference for writing Mire code right now, not as a historical overview of older syntax.

## Current Status

- Blocks use `{}` only.
- Legacy block syntax with `>` and `<` is obsolete and should be treated as unsupported.
- `import` is the only supported import keyword.
- `use` is the statement used to execute effectful calls or expressions.
- The stable core is intentionally small.
- Enums, pipelines, and expression-oriented control-flow forms should currently be treated as experimental or under review.
- Some older grammar paths still exist internally and will be removed in later cleanups.

## Toolchain Notes

- `mire debug` persists LLVM IR to disk for inspection.
- `mire run` and `mire build` keep LLVM IR in memory and persist only the binary plus incremental-cache metadata.
- Logical operators currently tolerate unresolved identifiers as `Unknown` operands during type inference, matching historic Avenys behavior.

## Minimal File

```mire
import std

pub fn main: () {
    use dasu(Hello Mire)
}
```

## Comments

Comments use explicit delimiters.

```mire
\! short comment !\

\!
multiline
comment
!\
```

## Blocks

Every block in Mire uses braces.

```mire
if cond {
    use dasu(ok)
} else {
    use dasu(no)
}
```

This applies to:

- functions
- `if`, `elif`, `else`
- `while`
- `for`
- `do { ... } while cond`
- `match`
- `struct`
- `type`
- `trait`
- `skill`
- `impl`
- `code`
- `enum`

## Imports

### Standard Modules

```mire
import std
import math
import strings
import fs as fs
import cpu as cpu
import strings: (split replace trim)
import proc as proc: (run shell)
```

### Local Imports

```mire
import ./utils
import ./utils/helpers
import ./utils/helpers: (parse format)
```

Rules:

- `import module` imports a standard or project-visible module.
- `as` defines an alias for standard modules.
- `: (a b c)` is the stable member-selection form.
- Local imports must start with `./`.
- Local imports do not support aliasing.
- Local imports use the original file name as their surface name.
- `import std` exposes the standard builtin surface, including qualified calls such as `std.input`.

## Variables and Assignment

Declarations use `set`.

```mire
set x = 10 :i64
set name = "mire"
set flag = true :bool
set xs = [1 2 3] :arr[i64 3]
set dyn = [] :vec![i64]
```

Mutable bindings are explicit.

```mire
set counter = 0 :i64 mut
set counter = counter + 1
set counter += 1
set counter -= 1
set counter *= 2
set counter /= 2
set counter %= 2
```

Notes:

- Bindings are immutable by default.
- `mut` enables reassignment.
- `const` still exists in internal representations, but the practical surface model is “immutable unless marked `mut`”.
- Member assignment is also supported:

```mire
set self.name = "new"
set user.age = 21
```

## Functions

```mire
fn greet: () {
    use dasu(Hello)
}

fn sum: (a:i64 b:i64) :i64 {
    return a + b
}

pub fn main: () {
    use greet()
    set result = sum(5 3) :i64
    use dasu(Result: {result})
}
```

Rules:

- Function syntax is `fn name: (params) :return_type { ... }`.
- If no return type is declared, the practical default is `none`.
- `pub` and `priv` are the visible surface visibility modifiers.
- `main` is the normal entry point.

## Parameters

```mire
fn scale: (x:i64 factor:i64) :i64 {
    return x * factor
}
```

Named arguments are also accepted in constructs that consume them:

```mire
set user = User(name="Evelyn" age=20)
```

## `use`

`use` evaluates an expression, typically when the expression has side effects.

```mire
use dasu(Hello)
use greet()
use fs.write("out.txt" "content")
```

It also appears naturally inside pipelines:

```mire
use range(5) => dasu({self})
```

## Expressions

### Literals

```mire
10
3.14
"hello"
true
false
none
```

### Identifiers and Member Access

```mire
x
total
user.name
```

### Binary Operators

```mire
a + b
a - b
a * b
a / b
a % b

a == b
a != b
a > b
a < b
a >= b
a <= b

a and b
a or b
```

### Unary Operators

```mire
-x
not ready
&value
*ptr
```

### Conversion and Type Inspection

```mire
str(x)
int(x)
float(x)
bool(x)
type x
x is(y)
x of i64
```

### Indexing and Access

```mire
user.name
arr at 0
(matrix at 1) at 2
```

## Pipelines

Pipelines are currently experimental.

```mire
set y = x => len()
use range(5) => dasu({self})
```

Current notes:

- The current behavior is intentionally considered too implicit or “magical” to be treated as frozen syntax.
- A form like `x => len()` is not yet documented as definitively meaning either `len(x)` or `x.len()`.
- `=>?` also remains experimental and its long-term purpose is still under review.
- Future design may keep only `=>`, refine both `=>` and `=>?`, or replace them with a smaller and clearer pipeline model.

## Strings and Interpolation

`dasu(...)` and template-friendly output forms accept free text plus interpolation.

```mire
use dasu(Hello {name})
use dasu(Total: {x + 2})
use dasu("Hello {name}!")
```

Notes:

- `{expr}` interpolates an expression.
- `dasu(...)` accepts unquoted free text in addition to normal strings.
- Regular string literals use double quotes.

## Input and Output

```mire
use dasu(Hello)

set name = std.input(: )
set age = std.input(: ) :i64
```

Practical built-ins:

- `dasu(...)`
- `ireru(...)`
- `std.input(...)`
- `std.output(...)`

## Conditionals

### `if` Statement

```mire
if x > 10 {
    use dasu(Greater)
} elif x == 10 {
    use dasu(Equal)
} else {
    use dasu(Lower)
}
```

### `if` Expression

```mire
set y = if x > 1 { 10 } else { 20 } :i64
```

Status:

- `if` as an expression is currently experimental.
- It is possible today, but it is not considered part of Mire's fully settled surface model yet.
- A possible long-term Mire-specific direction is expression flow driven through explicit data passing or pipelines, for example:

```mire
set x => if self > 0 {
    ...
} elif self == 0 {
    ...
} else {
    ...
}
```

## Loops

### `while`

```mire
set i = 0 :i64 mut

while i < 5 {
    use dasu({i})
    set i += 1
}
```

### `for`

```mire
for i in range(10) {
    use dasu({i})
}

for k, v in items {
    use dasu({k} {v})
}
```

### `do-while`

```mire
set count = 0 :i64 mut

do {
    set count += 1
} while count != 10
```

### Flow Control

```mire
break
continue
```

## Match

### Match Expression

```mire
set result = match x {
    1 { 10 }
    5 { 20 }
    _ { 0 }
} :i64
```

### Match Statement

```mire
match code {
    200 {
        use dasu(ok)
    }
    _ {
        use dasu(error)
    }
}
```

Notes:

- `_` is the wildcard arm.
- Patterns are currently simple literals or identifiers.
- The type checker validates the matched value and arm results or bodies.
- Pattern identifiers are treated as patterns, not as normal program expressions.
- `match` statements are part of the stable control-flow surface.
- `match` as an expression is available today and useful, but enum-oriented pattern matching is still evolving.

## Types

### Primitive Types

- `i8`, `i16`, `i32`, `i64`
- `u8`, `u16`, `u32`, `u64`
- `f32`, `f64`
- `str`
- `bool`

### Collection and Compound Types

```mire
arr[i64 4]
vec[i64]
vec![i64]
map[str i64]
tuple
```

Notes:

- `arr[T N]` is fixed-size.
- `vec[T]` is a typed vector form.
- `vec![T]` is a dynamic vector form.
- `map[K V]` is the main typed map form.
- `tuple` should currently be treated as experimental.
- Older internal or transitional type names such as `dict`, `list`, `ref`, `refmut`, and similar surfaces are not part of the frozen public syntax and are expected to disappear in later cleanup passes.

## Collection Literals

### Sequences

```mire
set xs = [1 2 3] :arr[i64 3]
set ys = [] :vec![i64]
set nested = [[1 2] [3 4]] :vec[vec[i64]]
```

### Maps and Dict-like Braces

```mire
set m = {a: 1, b: 2} :map[str i64]
```

Recommendation:

- Prefer `{k: v}` for map-like data in new code.

## Structs and Types

`struct` and `type` currently share the same basic surface grammar.

```mire
struct User {
    name :str
    age :i64
}

type Point {
    x :i64
    y :i64
}
```

Surface inheritance is also present:

```mire
struct Admin extends User {
    role :str
}
```

Construction:

```mire
set user = User(name="Evelyn" age=20)
```

Status:

- Basic object construction should currently be treated as debug-stage but directionally important.
- The intended construction shape is:

```mire
set obj = Obj(X=Value Y=Value Z=Value)
```

- This area still needs clearer freezing, especially once enums and payload-bearing objects settle.

## Traits, Skills, Impl, and Code

### Trait

```mire
trait Display {
    fn show: () :str
}
```

### Skill

```mire
skill Drawable {
    fn draw: () :none
}
```

### Impl

```mire
impl User {
    fn greet: () {
        use dasu(Hello {self.name})
    }
}

impl Display for User {
    fn show: () :str {
        return "User"
    }
}
```

Notes:

- Mire's modern OOP direction is intentionally small:
  - `struct`
  - `skill`
  - `impl`
- In several method contexts, `self` is inserted implicitly by the parser.
- Older ideas such as `code` are legacy design experiments and should be considered under review rather than part of the preferred model.

## Enums

```mire
enum Result[T] {
    Ok { V :T }
    Err { V :T }
    Empty
}
```

Notes:

- Enums are experimental.
- The currently intended direction is closer to:

```mire
set r = Result.Ok(V=x)
set empty = Result.Empty
```

- The intended matching direction is also enum-qualified patterns:

```mire
match r {
    Result.Ok { value } {
        value
    }
    Result.Err { error } {
        0
    }
    Result.Empty {
        -1
    }
}
```

- The exact enum syntax and payload conventions are not frozen yet.

## References, Pointers, and Box

```mire
set x = 2 :i64
set rx = &x
set bx = box[i64]
```

Dereference is also supported:

```mire
*ptr
```

Notes:

- Mire keeps a single ownership-oriented design model.
- `&T` is an immutable borrow.
- `&mut T` is a mutable borrow.
- `box[T]` is a heap-owned value.
- `*T` is a pointer surface.
- Older explicit declaration forms such as `ptr`, `ref`, and `refmut` are considered obsolete and are expected to be removed in later cleanup.
- The ownership model should remain small, functional, and safe first; extra layers can be added only after the base is stable and tested.

## Closures

Closures exist internally and are used by parser desugarings such as `if` expressions and `do-while`.

Mire does not yet expose a surface lambda syntax that is as stable as the rest of the language, so closures should currently be treated as an implementation detail rather than a primary user-facing construct.

## Standard Modules

Mire currently exposes standard modules through `import`.

Visible modules today:

- `math`
- `strings`
- `lists`
- `dicts`
- `time`
- `term`
- `mem`
- `cpu`
- `gpu`
- `fs`
- `env`
- `proc`

Examples:

```mire
import strings
import fs as fs
import proc as proc
```

Known members in the current type-checker surface:

### `math`

- `abs`
- `min`
- `max`
- `sum`
- `range`
- `round`
- `floor`
- `ceil`
- `clamp`

### `strings`

- `upper`
- `lower`
- `strip`
- `split`
- `replace`
- `contains`
- `startswith`
- `endswith`
- `len`
- `trim`
- `ltrim`
- `rtrim`
- `substr`
- `pad_left`
- `pad_right`
- `repeat`
- `is_empty`

### `lists`

- `len`
- `push`
- `pop`
- `remove`
- `delete`
- `append`
- `clear`
- `join`
- `contains`
- `index_of`
- `first`
- `last`
- `slice`
- `concat`
- `flatten`
- `reverse`
- `sort`
- `unique`
- `is_empty`

### `dicts`

- `len`
- `keys`
- `values`
- `has`
- `get`
- `set`
- `remove`
- `delete`
- `entries`
- `merge`
- `is_empty`

### `time`

- `unix_ms`
- `unix_ns`
- `since_ms`
- `since_ns`
- `mark`
- `elapsed_ms`
- `elapsed_ns`
- `sleep_ms`
- `sleep_ns`

### `term`

- `print`
- `println`
- `style`
- `hr`
- `clear`
- `input`

### `mem`

- `used`
- `total`
- `free`
- `available`
- `percent`
- `process`
- `snapshot`
- `format`

### `cpu`

- `time_ns`
- `time_ms`
- `mark`
- `elapsed_ns`
- `elapsed_ms`
- `count`
- `freq_mhz`
- `cycles_est`
- `loadavg`
- `snapshot`

### `gpu`

- `available`
- `snapshot`

### `fs`

- `read`
- `write`
- `append`
- `exists`
- `size`
- `copy`
- `move`
- `drop`
- `list`
- `mkdir`
- `rmdir`
- `join`
- `dir`
- `name`
- `ext`

### `env`

- `get`
- `set`
- `all`
- `args`
- `cwd`
- `chdir`

### `proc`

- `run`
- `spawn`
- `pipe`
- `shell`
- `read`
- `write`
- `on`
- `exit`
- `err`
- `exec`
- `exec_bg`
- `kill`
- `wait`
- `exists`

For more runtime-oriented detail, see [runtime-modules.md](./runtime-modules.md).

## Experimental or Under Review

Some grammar paths exist but should not be treated as frozen public syntax yet:

- `class`
- `module`
- `unsafe`
- `asm`
- `extern lib`
- `extern fn`
- `drop`
- `move`
- `query`
- `dmire_table`
- `dmire_column`
- `dmire_dlist`
- `find`
- `code`
- enum payload and pattern surface
- pipelines
- `if` expressions as a language-design feature

Notes:

- Some of these may eventually be implemented properly.
- Some are old planning artifacts or abandoned experiments.
- Some, such as the `dmire_*` family, are effectively obsolete unless revived deliberately.

## Recommendations for New Code

- Use `{}` for every block.
- Use `import`, never `add`.
- Prefer `match` with brace-based arms.
- Prefer `{k: v}` for map-like literals.
- Use explicit type annotations when collection shape matters.
- Treat enums, pipelines, `if` expressions, `code`, `class`, `query`, and the `dmire_*` family as experimental or under review.

## Complete Example

```mire
import std
import fs as fs

struct User {
    name :str
    age :i64
}

impl User {
    fn greet: () {
        use dasu(Hello {self.name}, age {self.age})
    }
}

pub fn main: () {
    set user = User(name="Evelyn" age=20)
    use user.greet()

    set nums = [1 2 3] :arr[i64 3]
    set count = nums => len()
    use dasu(count: {count})

    match count {
        3 {
            use dasu(exact)
        }
        _ {
            use dasu(other)
        }
    }
}
```

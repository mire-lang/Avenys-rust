# Mire

**Version 1.0.0**

Mire is a compiled, statically typed programming language with an ownership-oriented memory model. Version 1.0.0 marks a complete syntax break from all prior versions of the language. No code written for Avenys or pre-1.0 Mire is compatible with this release.

---

## What this version actually provides

This section is intentionally honest. V1.0.0 is a working compiler with a real type checker, a real ownership checker, and a real standard library surface — but not every feature that appears in the syntax reference is fully guaranteed at the compiler level yet. The distinction matters.

### What the compiler fully checks and enforces

**Type checking** (`typeck.rs`)

- Type inference for variable declarations: `set x = 10` infers `i64` without an annotation
- Type inference across binary expressions: arithmetic, comparison, and logical operators all resolve correctly
- Function return type inference and return type mismatch detection
- Assignment type mismatch errors: assigning `str` to an `i64` binding is a hard error
- Undefined identifier errors at the use site
- `match` arm type consistency: all arm patterns must be compatible with the matched value's type
- Loop variable type inference: `for i in range(10)` gives `i` type `i64`; iterating over a typed array or vector infers the element type
- `if` and `while` conditions are checked to be bool-like; a condition of type `i64` is an error
- Function call return type propagation: calling a known function resolves the call expression's type
- All standard library modules (`math`, `strings`, `lists`, `dicts`, `time`, `term`, `mem`, `cpu`, `gpu`, `fs`, `env`, `proc`) are registered with known member return types
- Builtin functions (`dasu`, `len`, `range`, `str`, `int`, `float`, `bool`, `input`, etc.) have registered return types and are accepted without errors

**Ownership and borrow checking** (`borrowck.rs` / MSS)

- Use-after-move detection: using a binding after it has been explicitly moved is a hard error
- Move-while-borrowed: moving a value that currently has an active borrow is rejected
- Shared borrow exclusivity: taking a mutable reference while a shared borrow is active is rejected
- Multiple mutable references: a second `&mut` to the same binding is rejected
- Mutation-while-shared: writing to a binding that has active shared borrows is rejected
- Drop-while-borrowed: explicitly dropping a borrowed binding is rejected
- Borrow lifetime: borrows are automatically released when their scope ends; post-scope writes to the original owner are permitted
- Return-of-local-reference: returning a reference to a locally scoped binding is a hard error ("borrow outlives owner scope")
- Call argument checking: passing a shared reference to a function that expects a mutable reference is rejected
- Move semantics by type: `str` and non-primitive types consume the binding on pass-by-value; numeric primitives (`i64`, `f32`, etc.) are copy-like and do not
- `unsafe` blocks bypass borrow conflict checks explicitly, as documented

**Semantic analysis** (`semantic.rs`)

- Scope tree construction: every block creates a child scope with a stable ID
- Binding registration with scope depth and kind (`Value`, `SharedRef`, `MutableRef`, `Boxed`, `Parameter`)
- Function signature collection with param types and return types
- Borrow fact recording for all `&` and `&mut` expressions
- Move fact and drop fact recording

---

### What exists in the parser but is not fully guaranteed

The following constructs parse without errors but the compiler does not currently apply deep type or ownership analysis to them. They may work in practice depending on what you write, but they are not guaranteed:

- **`struct` and `type` construction** — object creation (`User(name="Evelyn" age=20)`) is parsed, and type signatures are collected by the type checker, but field-level type checking during construction is not enforced
- **`impl` and method calls** — method resolution exists in the semantic model but method bodies are not checked against `self`'s field types
- **`enum` declarations and matching** — enum variants parse and the type checker skips them (`Statement::Enum` is a no-op in both `typeck.rs` and `borrowck.rs`); enum-qualified patterns are not validated
- **Pipelines (`=>`)** — pipelines are walked by both the type checker and borrow checker but their semantics are not fully resolved; `x => len()` may or may not behave as `len(x)` depending on the runtime
- **`trait` and `skill` declarations** — registered in the type checker's scope but methods are not checked for conformance
- **`if` as an expression** — parsed and desugared via `__if_expr` builtin; return type is `Unknown`, not unified from branches
- **`extern lib` and `extern fn`** — parsed, walked past in both checkers without analysis
- **`unsafe`, `asm`, `module`** — scopes are created and walked, but the content is not semantically validated beyond what falls inside the normal expression checker

---

## Syntax

All blocks use `{}`. The `>` / `<` block syntax from Avenys is gone entirely.

### Minimal program

```mire
import std

pub fn main: () {
    use dasu(Hello Mire)
}
```

### Variables

```mire
set x = 10 :i64
set name = "mire"
set flag = true :bool

set counter = 0 :i64 mut
set counter += 1
```

Bindings are immutable by default. `mut` enables reassignment. Annotations are optional when the type can be inferred.

### Functions

```mire
fn sum: (a:i64 b:i64) :i64 {
    return a + b
}

pub fn main: () {
    set result = sum(5 3) :i64
    use dasu(Result: {result})
}
```

`use` evaluates an expression for its side effects. `pub` / `priv` control visibility.

### Control flow

```mire
if x > 10 {
    use dasu(greater)
} elif x == 10 {
    use dasu(equal)
} else {
    use dasu(lower)
}

while i < 5 {
    set i += 1
}

for i in range(10) {
    use dasu({i})
}

do {
    set count += 1
} while count != 10
```

### Match

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

`_` is the wildcard arm. Patterns are currently literal values or identifiers; enum-qualified patterns are not yet enforced.

### Types

Primitive: `i8` `i16` `i32` `i64` `u8` `u16` `u32` `u64` `f32` `f64` `str` `bool`

Collections:

```mire
set xs  = [1 2 3]      :arr[i64 3]   \! fixed-size !\
set ys  = []           :vec![i64]    \! dynamic vector !\
set m   = {a: 1, b: 2} :map[str i64]
```

### Structs

```mire
struct User {
    name :str
    age  :i64
}

impl User {
    fn greet: () {
        use dasu(Hello {self.name})
    }
}

set user = User(name="Evelyn" age=20)
use user.greet()
```

Construction and method dispatch are parsed and run, but field-level type checking during construction is not enforced yet (see above).

### Ownership

```mire
set x  = 2 :i64
set rx = &x          \! shared borrow !\
set bx = box[i64]    \! heap-owned !\
```

The borrow checker enforces the rules described in the "What the compiler fully checks" section above. `unsafe` blocks are the explicit escape hatch.

### Imports

```mire
import std
import math
import fs as fs
import strings: (split replace trim)
import ./utils
```

### Comments

```mire
\! short comment !\

\!
multiline
comment
!\
```

---

## Standard library

Modules available via `import`: `math`, `strings`, `lists`, `dicts`, `time`, `term`, `mem`, `cpu`, `gpu`, `fs`, `env`, `proc`.

All members of these modules are registered in the type checker. Return types are known for the majority of members; some return `Anything` where the type is collection-generic or polymorphic.

For full member listings see [syntax-V1.0.0.md](./syntax-V1.0.0.md).

---

## What is experimental or under review

The following should not be treated as stable surface in 1.0.0:

- Enums and enum-qualified pattern matching
- Pipelines (`=>` and `=>?`)
- `if` as an expression
- `tuple` type
- `class`, `module`, `unsafe`, `asm`, `extern lib`, `extern fn`
- `drop`, `move` as explicit statements (they parse and run but are closer to internal primitives than user-facing constructs)
- The `dmire_*` family (`dmire_table`, `dmire_column`, `dmire_dlist`) — obsolete unless deliberately revived
- `query` and `find` — exist in the AST and borrow checker but are not semantically validated

---

## Project structure

```
src/
  avens/
    mod.rs
    runtime_sup.rs
  compiler/
    borrowck.rs     — ownership and borrow checker
    mod.rs
    semantic.rs     — scope and binding model
    typeck.rs       — type inference and type checking
  error/
    mod.rs
    mss.rs          — MSS (Memory Safety System) error types
  lexer/
    mod.rs
  parser/
    ast.rs
    lib.rs
    loader.rs
    main.rs
```

---

## Migration from Avenys / pre-1.0 Mire

V1.0.0 is a hard break. Nothing from previous versions is source-compatible. Key changes:

- All blocks now use `{}` exclusively. The `>` / `<` block delimiters are gone
- `add` is no longer a valid import keyword; use `import`
- `use` is now the statement for effectful expression evaluation, not an import keyword
- The type annotation position has changed; annotations follow the binding or parameter name with `:`
- Mutability is now declared with `mut` on the binding, not on the type

---

## Version

`1.0.0` — first stable syntax release. Compiled from the Avenys 0.x codebase with a full parser rewrite and a rewritten `typeck.rs` following a corruption event during development. The semantic and borrow checking layers are original to this release.
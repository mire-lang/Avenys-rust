# Mire Syntax Reference

> Reference manual for the Mire programming language syntax.

---

## 1. Comments

```
// single-line comment
!/ block comment !/
```

---

## 2. Literals

| Type | Syntax | Examples |
|------|--------|----------|
| Integer (decimal) | `[0-9]+` | `42`, `0` |
| Integer (binary) | `0b[01]+` | `0b1010` |
| Integer (octal) | `0o[0-7]+` | `0o12` |
| Integer (hex) | `0x[0-9a-fA-F]+` | `0xFF` |
| Negative integer | `-[digit]...` | `-17` |
| Float | `[0-9]+.[0-9]+` | `3.14`, `-2.0` |
| Char | `'x'` | `'a'`, `'\n'`, `'ñ'` |
| String | `"..."` | `"hello"`, `"line\nbreak"` |
| Raw string | `r"..."`, `r#"..."#` | `r"hello \"world\""` |
| Boolean | `true` / `false` | `true` |
| Unit/null | `mu` | `mu` |
| List | `[elem1 elem2]` | `[1 2 3]`, `[]` |
| Dict (bracket) | `[k1 v1 k2 v2]` | `[a 1 b 2]` |
| Dict (brace) | `{k: v, k: v}` | `{a: 1, b: 2}` |
| Tuple | `(e1, e2)` | `(1, "hello")` |

String escape sequences: `\n`, `\r`, `\t`, `\\`, `\"`, `\'`, `\{`, `\}`

String interpolation (in `use dasu`/`use ireru`): `{expr}` inside string literals, `{expr:spec}` with format spec. Use `{{` and `}}` for literal braces.

---

## 3. Keywords

`set`, `use`, `fn`, `return`, `if`, `elif`, `else`, `while`, `for`, `in`, `find`, `do`, `match`, `type`, `struct`, `skill`, `impl`, `enum`, `extern`, `lib`, `unsafe`, `asm`, `load`, `load!`, `module`, `new`, `own`, `move`, `drop`, `pub`, `priv`, `const`, `mut`, `self`, `break`, `continue`, `mu`, `true`, `false`

---

## 4. Operators

### Arithmetic
`+` (add/concat), `-` (sub/neg), `*` (mul), `/` (div), `%` (mod)

### Comparison
`==`, `!=`, `>`, `<`, `>=`, `<=`

### Logical
`&&`, `||`, `!`, `^`

### Bitwise
`&`, `|`, `^`, `<<`, `>>`

### Assignment
`=`, `+=`, `-=`, `*=`, `/=`, `%=`

### Special
`.` (member access), `::` (namespace), `@` (index alternative), `?` (try), `is` (type check), `in` (containment), `to` (range), `=>` (pipeline/closure arrow), `=>?` (safe pipeline)

---

## 5. Type Syntax

### Primitives
`i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`, `f32`, `f64`, `char`, `str`, `bool`, `mu`

### Compound
| Syntax | Meaning |
|--------|---------|
| `vec[T]` | Dynamic vector |
| `arr[T SIZE]` | Fixed-size array |
| `slice[T]` | Slice |
| `map[K V]` | Map/dictionary |
| `result[T E]` | Result type |
| `&T` | Shared reference |
| `&mut T` | Mutable reference |
| `*const T` / `*mut T` | Pointer (FFI) |

### User-defined
`StructName`, `EnumName`, `Module::TypeName`, generic `Name[T]`, `Name[T: Bound]`

### Type ascription
`(expr :Type)` -- explicit type annotation: `42 :i64`, `[1 2 3] :vec[i64]`

---

## 6. Statements

### Variable declaration / assignment
```
set name = expr                    // inferred type
set name = expr :Type              // explicit type
set name = expr :Type mut          // mutable
set name = expr :Type const        // constant
set field.path = expr              // field assignment
set arr@idx = expr                 // index assignment
set x += 1                         // compound assignment
```

`set` performs a new binding if the variable is not yet declared, otherwise reassignment.

### Function definition
```
fn name: (param1 :Type1 param2 :Type2) :ReturnType {
    body
}

pub fn name[T]: (x :T) :T {
    body
}

fn name[T: Skill1 Skill2]: (x :T) :T {
    body
}
```

The `:` after the function name is required. Parameters are space-separated.

### Return
```
return expr
return
```

### If / elif / else
```
if condition {
    body
} elif condition2 {
    body
} else {
    body
}
```

### If expression
```
set result = if condition { val_if_true } else { val_if_false }
```

### While loop
```
while condition {
    body
}
```

### Do-while loop
```
do {
    body
} while condition
```

### For loop
```
for item in iterable {
    body
}

for item, index in iterable {
    body
}
```

### Find statement
```
find variable in iterable {
    body
}
```

### Match statement / expression
```
match value {
    pattern1 { body }
    pattern2 when guard { body }
    pat1 | pat2 { body }
    1..5 { body }
    _ { default }
}
```

Pattern types: literals, identifiers (bindings), `EnumName.Variant(p1 p2)`, `_` (wildcard), range `1..5`, alternatives `pat1 | pat2`.

### Type/struct definition
```
type TypeName {
    field1 :Type1
    field2 :Type2 mut
}

pub type TypeName[T] {
    field :T
}
```

### Enum definition
```
enum EnumName {
    Variant1
    Variant2(payload :Type)
    Variant3(name1 :Type1, name2 :Type2)
}

enum EnumName[T] {
    Some(T)
    None
}
```

### Skill (trait) definition
```
skill SkillName {
    fn method1: (self param :Type) :ReturnType
    fn method2: (param :Type) :ReturnType
}
```

### Impl block
```
impl TypeName {
    fn method: (self) :ReturnType { body }
}

impl SkillName for TypeName {
    fn method: (self) :ReturnType { body }
}

impl[T] TypeName[T] {
    fn method: (self) :ReturnType { body }
}
```

### Extern (FFI)
```
extern lib "name" "path.so"
extern fn function_name: (param1 :*const i8 param2 :i32) :i32 lib "name"
```

### Unsafe block
```
unsafe {
    body
}
```

### Inline assembly
```
asm {
    mov rax, rbx
}
```

### Import
```
load package                        // from owl.toml dependency
load package::submodule
load package as alias

load! path/to/module                // local file (relative)
load! /path/to/module               // local file (absolute)
```

### Module declaration
```
module ModuleName
```

### Use (call imported function / builtin)
```
use dasu("hello")
use dasu("value: {x}")
use ireru("error")
use module::function(args)
use! module::function(args)
```

### Lifecycle
```
new::(args) :Type
own::(value) :Type
move::(value) to target
drop::(value)
```

---

## 7. Expressions

All literals, identifiers, binary/unary operations, function calls, method calls, indexing, closures, pipelines, match expressions, if expressions, type ascription, try `?`, ok/err, enum variant construction, box.

### Function calls
```
function_name(arg1 arg2 arg3)
function_name(name1: val1 name2: val2)
Module::function(arg1 arg2)
```

Arguments are space-separated (no commas required).

### Closures
```
(expr) => expr
(param1 param2) => expr
(x: i64) => x * 2
(param1 param2) => {
    statement1
    return result
}
```

### Pipeline
```
input => stage
input =>? safe_stage
```

### Indexing
```
collection@0
collection[index]
```

---

## 8. Attributes

```
@[test]
fn test_something: () { ... }

@[test][section("math")]
fn test_math: () { ... }

@[test, section("math")]
fn test_math: () { ... }
```

---

## 9. Visibility

- `pub` -- public (exported)
- `priv` -- private (explicit default)
- No modifier = private by default

---

## 10. Generics

### On functions
```
fn name[T]: (x :T) :T { ... }
fn name[T: Skill1 Skill2]: (x :T) :T { ... }
```

### On types
```
type Name[T] { field :T }
enum Name[T] { Variant(T) }
```

### On impl
```
impl[T] Name[T] { ... }
```

### Type arguments at call site
```
name[Type1 Type2](args)
Name[Type]::method(args)
```

---

## 11. Named Arguments (Struct Construction)

```
TypeName(field1: val1, field2: val2)
EnumName.VariantName(name1: val1, name2: val2)
```

---

## 12. Scope Rules

- Variables declared with `set` are scoped to the current block `{ }`
- Functions are declared at their point of definition
- Type and enum names must be declared before use
- `mut` must be re-declared on each `set` for mutable variables
- Match pattern bindings are visible inside the case body

---

## 13. Files and Entry Point

- Source files use the `.mire` extension
- Entry point is `pub fn main: () { ... }`
- Module files declare `module ModuleName` at the top

---

## Key Conventions

1. Functions use `:` after the name (not parentheses): `fn name: (params) :ReturnType { body }`
2. Arguments are space-separated: `add(2 3)`, `fn(a :i64 b :i64) :i64`
3. Type ascription comes after expressions: `set x = 42 :i64`
4. `set` serves as both `let` (new binding) and reassignment
5. `use` is used to call builtins and imported functions
6. Pipeline `=>` replaces `self` in the right-hand expression
7. The `@` operator is an alternative to brackets for indexing: `arr@0`

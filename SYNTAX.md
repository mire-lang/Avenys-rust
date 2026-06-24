# Mire Language Reference

Version: **3.11.31** · 285 tests passing

---

## 1. Your first program

```mire
pub fn main: () {
    use dasu("Hello, Mire!")
}
```

Save as `hello.mire` and run:

```bash
mire run hello.mire
```

---

## 2. Bindings and mutability

```mire
set age = 25 :i64          # immutable binding with explicit type
set name = "mire"          # type inferred as :str
set ready = true           # inferred :bool
set total = 0 :i64 mut     # mutable — can be reassigned
set pi = 3.14159 :f64 const # compile-time constant

# Reassigning a mutable binding
set total = total + 10

# Compound assignment
set total += 5             # same as: set total = total + 5
set total -= 2
set total *= 3
set total /= 4
set total %= 2
```

Rules:
- `set` declares a binding
- `name :Type` annotates the type
- `mut` enables reassignment
- `const` marks a compile-time constant
- `name :Type mut` combines type annotation with mutability

---

## 3. Comments

```mire
# Line comment
// Also a line comment

//! This is a
    block comment
    spanning multiple lines !//
```

---

## 4. Functions

```mire
# Simple function with return type
fn add: (a: i64, b: i64) :i64 {
    return a + b
}

# No return value (returns mu)
fn greet: (name: str) {
    use dasu("Hello, {name}")
}

# Entry point
pub fn main: () {
    set result = add(5, 3)
    use dasu(str(result))
}
```

Functions are private by default. Use `pub` to export.

### Closures

```mire
set double = (x: i64) => x * 2
set result = double(21)    # 42

# Multi-line closures use braces
set clamp = (val: i64, min: i64, max: i64) => {
    if val < min { return min }
    if val > max { return max }
    return val
}
```

### Generics

```mire
fn identity[T]: (x: T) :T {
    return x
}

set a = identity[i64](42)   # explicit type argument
set b = identity("ok")      # inferred T = str
```

---

## 5. Structs

```mire
struct Point {
    x: i64
    y: i64
}

# Construction
set p = (Point x: 1, y: 2)

# Field access
set px = p.x

# Mutation (only if field has 'mut')
struct Counter {
    value: i64 mut
    step: i64
}
set c = (Counter value: 0, step: 1)
set c.value = c.value + c.step
```

### Methods (`impl`)

```mire
impl Point {
    # Instance method — takes self
    fn sum: (self) :i64 {
        return self.x + self.y
    }

    # Associated method — called with ::
    fn origin: () :Point {
        return (Point x: 0, y: 0)
    }
}

set p = Point::origin()
set total = p.sum()
```

---

## 6. Enums

```mire
enum Color { Red, Green, Blue }

enum Option[T] {
    None
    Some(value: T)
}

# Construction
set c = Color.Red
set o = Option[i64].Some(42)

# Pattern matching
match o {
    Option.None        { use dasu("nothing") }
    Option.Some(v)     { use dasu("got {v}") }
}

# Match with return value
set label = match c {
    Color.Red   { "red" }
    Color.Green { "green" }
    Color.Blue  { "blue" }
} :str
```

---

## 7. Skills (Traits)

Skills define abstract contracts — method signatures with no bodies. Types
implement skills via `impl ... for ...` blocks. This is Mire's trait system.

### Declaring a skill

```mire
pub skill Show {
    fn show: (self) :str
}

pub skill Size {
    fn size: (self) :i64
}
```

A skill must have at least one method. `self` (if present) must be the first
parameter.

### Implementing a skill

```mire
struct Point {
    x: i64
    y: i64
}

impl Show for Point {
    fn show: (self) :str {
        return "Point({self.x}, {self.y})"
    }
}

impl Size for Point {
    fn size: (self) :i64 { return self.x + self.y }
}
```

The type must provide a method for EVERY signature declared in the skill.

### Self-type annotation

Methods can require a specific `self` type:

```mire
pub skill Projectable {
    fn project: (self: Point) :i64
}

impl Projectable for Point {
    fn project: (self) :i64 { return self.x }
}
```

### Multiple skills on one type

A single type can implement any number of skills:

```mire
pub skill Greeter {
    fn greet: (self) :str
}

pub skill Counter {
    fn count: (self) :i64
}

struct Person { name :str, age :i64 }

impl Greeter for Person {
    fn greet: (self) :str { return "Hi, " + self.name }
}

impl Counter for Person {
    fn count: (self) :i64 { return self.age }
}
```

### Generic trait bounds

Skills can constrain generic type parameters with `[T: SkillName]`:

```mire
skill Show {
    fn show: (self) :str
}

type Num { value :i64 }

impl Show for Num {
    fn show: (self) :str { return "num" }
}

fn print_it[T: Show]: (x: T) {
    use dasu(x.show())
}

pub fn main: () {
    set n = Num(1)
    print_it(n)                     # prints "num"
}
```

The compiler enforces that the concrete type argument implements all required
skills at the call site.

### Instance vs associated methods

Methods with `self` are **instance methods** — called with dot notation
(`x.method()`). Methods without `self` are **associated methods** — called
with `::` (`Type::method()`). The impl must match the skill declaration.

### Error: skill not implemented

```
error[E0008]: Type 'Foo' does not implement required method 'Show.show'
```

---

## 8. Control flow

```mire
# If / elif / else
if x > 10 {
    use dasu("big")
} elif x > 0 {
    use dasu("small")
} else {
    use dasu("zero or negative")
}

# While loop
set i = 0 :i64 mut
while i < 10 {
    set i += 1
}

# For loop (over ranges or collections)
for n in range(5) {
    use dasu(n)
}

# Do-while
do {
    set i += 1
} while i < 20

# Break and continue
while true {
    if done { break }
    if skip { continue }
    set i += 1
}
```

---

## 9. Collections

### Arrays (fixed size)

```mire
set arr = [10, 20, 30] :arr[i64 3]
set first = arr at 0          # index access
set arr at 1 = 99              # index mutation
```

### Vectors (dynamic)

```mire
set nums = [] :vec[i64] mut
lists.push(nums, 1)
lists.push(nums, 2)
set n = lists.get(&nums, 0)
set len = lists.len(&nums)
```

### Dicts / Maps

```mire
set m = {a: 1, b: 2} :map[str i64]
set val = dicts.get(m, "a")
dicts.set(m, "c", 3)
```

### Higher-order functions

```mire
load kioto

pub fn main: () {
    # Fold (reduce with accumulator)
    set sum = lists.fold(0, (acc, x) => acc + x, [1, 2, 3, 4, 5])

    # Map (transform every element)
    set doubled = lists.map((x) => x * 2, [1, 2, 3])

    # Filter (keep matching elements)
    set evens = lists.filter((x) => x > 2, [1, 2, 3, 4, 5])

    use dasu("sum={sum}")
}
```

---

## 10. Operators

### Arithmetic

```mire
+   -   *   /   %       # standard arithmetic
```

### Comparison

```mire
==   !=   <   >   <=   >=
```

### Logical

```mire
&&   ||   !             # and, or, not
```

### Bitwise

```mire
&   |   ^   <<   >>     # and, or, xor, shift-left, shift-right
```

### Pipeline

```mire
set result = [1, 2, 3]
    |> lists.map((x) => x * 2)
    |> lists.filter((x) => x > 2)
```

---

## 11. Types

### Primitives

| Type | Example | Notes |
|------|---------|-------|
| `i8`, `i16`, `i32`, `i64` | `42 :i64` | Signed integers |
| `u8`, `u16`, `u32`, `u64` | `42 :u32` | Unsigned |
| `f32`, `f64` | `3.14 :f64` | Floating point |
| `char` | `'a' :char` | Unicode scalar |
| `str` | `"hello" :str` | Heap string |
| `bool` | `true`, `false` | Boolean |
| `mu` | `set x = mu :mu` | Unit / void |

### Literal forms

```mire
set dec = 42                # decimal (inferred i64)
set hex = 0xFF              # hexadecimal
set bin = 0b1010            # binary
set oct = 0o77              # octal
set pi = 3.14159            # float (inferred f64)
set c = 'a'                 # char
set nl = '\n'               # escaped char
set s = "hello" :str        # string (requires :str)
set raw = r"no\nescapes"    # raw string
```

### Collections

| Type | Syntax | Example |
|------|--------|---------|
| Array | `arr[T N]` | `arr[i64 10]` |
| Vector | `vec[T]` | `vec[i64]` |
| Map | `map[K V]` | `map[str i64]` |

### References

| Type | Description |
|------|-------------|
| `&T` | Shared reference |
| `&mut T` | Mutable reference |

---

## 12. Ownership and references

```mire
set x = 42 :i64
set r = &x                  # shared reference
set v = *r                  # dereference

fn read: (value: &i64) :i64 {
    return *value
}

set y = read(&x)
```

The borrow checker enforces:
- No use-after-move
- No mutation while a shared borrow exists
- No multiple mutable references to the same value
- No returning references to local variables

---

## 13. Module system

Mire uses `load` to import modules. Dependencies are declared in `owl.toml`.
`use` executes expressions as statements (calls for side effects).

### `owl.toml`

```toml
[project]
name = "my-app"
version = "0.1.0"
entry = "main.mire"

[dependencies]
kioto = { path = "../kioto" }
```

### `load` — declare a package

```mire
load kioto                # load the entire standard library
load kioto::math          # load only the math subtree
load kioto::strings as s  # alias: s.upper() instead of strings.upper()
```

`load` must be at the top level of the file, never inside a function.

### `use` — expression statement (side-effect call)

```mire
use dasu("hello")         # output expression (built-in)
use proc::exit(1)          # side-effect call
use async::spawn("cmd")    # spawn background process
```

`use` evaluates an expression and discards the result. It is NOT an import
mechanism — use `load` exclusively for importing modules.

---

## 14. Kioto standard library

Kioto is auto-injected if not in `[dependencies]`. Available modules:

| Module | Key functions |
|--------|--------------|
| `strings` | `upper`, `lower`, `split`, `join`, `replace`, `trim`, `len`, `substr`, `contains`, `starts_with`, `ends_with` |
| `lists` | `push`, `pop`, `get`, `len`, `slice`, `concat`, `sort`, `reverse`, `unique`, `map`, `filter`, `fold` |
| `dicts` | `get`, `set`, `keys`, `values`, `has`, `len`, `remove`, `merge` |
| `math` | `sqrt`, `sin`, `cos`, `tan`, `pow`, `log`, `abs`, `min`, `max`, `pi`, `e`, `sum`, `mean`, `random` |
| `fs` | `read`, `write`, `exists`, `copy`, `move`, `delete`, `mkdir`, `list` |
| `env` | `get`, `set`, `args`, `cwd` |
| `proc` | `run`, `shell`, `spawn`, `pipe`, `on`, `err` |
| `async` | `spawn`, `join`, `ready`, `failed`, `task::done`, `task::error` |
| `time` | `now`, `sleep`, `format` |
| `net` | `connect`, `send`, `recv`, `close`, `poll`, `http::get`, `http::post`, `socket::nonblock` |
| `ws` | `connect`, `send::text`, `recv`, `recv::all`, `close` |
| `maybe` | `Some`, `None`, `map`, `unwrap` |
| `result` | `Ok`, `Err`, `map`, `unwrap`, `is_ok`, `is_err` |

---

## 15. I/O

```mire
# Output
use dasu("hello")          # print string + newline
use dasu(42)               # print integer
use dasu(3.14)             # print float
use dasu("x = {x}")        # string interpolation

# Input
set line = ireru()                   # read line from stdin
set name = ireru("Name: ")           # with prompt
set age = ireru("Age: ") :i64        # parse as i64
set height = ireru("Height: ") :f64  # parse as f64
```

---

## 16. String interpolation

```mire
set name = "Mire"
set count = 42
use dasu("Hello, {name}!")             # variable
use dasu("Result: {add(5, 3)}")        # expression
use dasu("Twice: {count * 2}")         # arithmetic
```

Anything inside `{}` is evaluated, converted to string with `str()`, and concatenated.

---

## 17. Unsafe, extern, assembly

### Unsafe blocks

```mire
unsafe {
    set raw = &x    # bypasses borrow checker for this block
}
```

### Extern functions

```mire
extern lib "c" "libc.so.6"
extern fn puts: (msg: *const i8) :i32 lib "c"
```

### Inline assembly

```mire
asm {
    mov rax, rbx
    add rax, 1
}
```

---

## 18. Tests

### Directives

```mire
#!cfg::test
pub fn test_arithmetic: () {
    assert_eq(add(2, 3), 5)
}

![test] string operations
pub fn test_strings: () {
    set s = strings.upper("hello")
    assert_eq(s, "HELLO")
}
```

### Running tests

```bash
cargo test                          # Rust integration tests
mire test                           # Mire source tests
mire test --verbose                 # per-test output
mire test tests/behavior/           # specific directory
```

### Assertions

```mire
assert_eq(actual, expected)
assert_ne(actual, expected)
```

---

## 19. CLI reference

```bash
mire run    [file] [--release] [-O<0|1|2|3|s|z>] [-- args...]
mire build  [file] [--release] [-O<0|1|2|3|s|z>]
mire check  [file] [--warn-all] [--deny <code>]
mire debug  [file] [--tokens] [--ast] [--ir]
mire test   [paths...] [--no-run] [--verbose]
mire validate                       # validate owl.toml
mire owl add <name> --path <path>   # add dependency
mire owl remove <name>              # remove dependency
```

---

## 20. Stability

**Stable and tested:**
- Structs, enums, pattern matching
- Skills (traits) and impl blocks, generic trait bounds
- Functions, closures, generics
- Type inference, borrow checking
- Collections (arrays, vectors, maps)
- All control flow (if, while, for, do-while, match)
- Module system (load, use)
- String interpolation
- I/O (dasu, ireru)
- Unsafe blocks, extern functions, inline assembly
- Incremental compilation with cache

**Improving:**
- Higher-order functions on lists (map/filter/fold — MIR lowering done, stubs in kioto)
- Type-safe unwrap for maybe/result
- Skill codegen (frontend/type-checking complete, LLVM backend pending)
- owl sync (package fetch/update command not yet implemented)

// Mire Language Syntax

> Topical reference for the Mire programming language syntax.

This section covers every syntactic construct in the Mire language. Each topic is organized into its own subdirectory with a comprehensive README covering explanation, examples, rules, and edge cases.

Use the table below to navigate to the specific area you need.

---

## Table of Contents

##// Core Concepts

| Topic | Description |
|-------|-------------|
| [Program Structure](program-structure/README.md) | Entry point, comments, attributes, modules |
| [Variables](variables/README.md) | Declaration, mutability, constants, scope |
| [Types](types/README.md) | Primitive, composite, type ascription, conversion |
| [Memory Ownership](memory-ownership/README.md) | Move semantics, borrow checker |

##// Functions & Closures

| Topic | Description |
|-------|-------------|
| [Functions](fn/README.md) | Declaration, calls, methods, generics, nested |
| [Closures](closures/README.md) | Anonymous functions, capture |
| [Pipeline](pipeline/README.md) | Pipeline operators `=>` and `?=>` |

##// Control Flow & Pattern Matching

| Topic | Description |
|-------|-------------|
| [Control Flow](control-flow/README.md) | if/else, while, for, find, do/while |
| [Pattern Matching](pattern-matching/README.md) | match expressions and statements |

##// Data Types

| Topic | Description |
|-------|-------------|
| [Structs & Inheritance](poo/README.md) | Structs, methods, `extends`, `impl` |
| [Enums](enums/README.md) | Enum declarations, payloads, matching |
| [Skills](skills/README.md) | Traits, `super` inheritance, generic bounds |
| [Generics](generics/README.md) | Generic functions, structs, trait bounds |
| [Collections](collections/README.md) | vec, map, array, Box |
| [Strings](strings/README.md) | String operations and methods |

##// Operators & Expressions

| Topic | Description |
|-------|-------------|
| [Operators](operators/README.md) | Arithmetic, comparison, bitwise, pipeline, precedence |
| [Error Handling](error-handling/README.md) | Result, Maybe, panic, `?` operator |

##// Module System & FFI

| Topic | Description |
|-------|-------------|
| [Modules](modules/README.md) | load, load!, exports, namespace access |
| [FFI](ffi/README.md) | C external function interface |
| [Macros](macros/README.md) | Macro system, `@[macro!]`, hygiene |

##// Builtins & Testing

| Topic | Description |
|-------|-------------|
| [Builtins I/O](builtins-io/README.md) | dasu, ireru, proc I/O |
| [Testing](testing/README.md) | `@[test]`, test framework, assertions |

---

## Quick Reference

```mire
// Functions
fn add: (a :i64, b :i64) :i64 { return a + b }

// Structs
struct Point { x :i64, y :i64 }
impl Point { fn dist: (self) :f64 { return sqrt((self.x * self.x + self.y * self.y) :f64) } }

// Enums
enum Option[T] { Some(value :T), None }

// Pattern matching
match opt { Some(v) => v, None => 0 }

// Vec
set v = [] :vec[i64] mut
set v = vec::push(v 42)

// Map
set m = {} :map[str i64]
set m = map::set(m "key" 42)

// Control flow
if x > 0 { dasu("positive") } else { dasu("non-positive") }
while i < 10 { set i += 1 }

// Module load
load kioto
load mire::vec

// FFI
extern fn puts: (msg :*const i8) :i32 lib "c"
```

---

## Syntax Rules

- Arguments at call sites are **space-separated** (commas tolerated)
- `} else {` must be on the **same line**
- All bindings are **immutable** unless marked `mut`
- `set` handles both declaration and reassignment
- Struct literals use `(...)` with space-separated `Field=value`
- Comments: `//` for line, `/! ... !/` for block
- `@[test]` marks unit tests; `@[macro!]` declares macros

---

## Related

- [Avenys Documentation Index](../README.md)
- [Avenys 4.0.0 Release Notes](../../avenys/docs/RELEASE-4.0.0.md)
- [Mire Language Reference](../../avenys/SYNTAX.md)

---

## License

GNU General Public License v3.0

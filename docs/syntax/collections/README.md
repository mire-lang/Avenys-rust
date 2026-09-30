// Collections

## Vectors (`vec[T]`)

Dynamic arrays, heap-allocated.

```mire
// Creation
set v = [] :vec[i64] mut
set v = [1 2 3] :vec[i64]  // Immutable literal

// Operations
vec::push(v, 4)                    // v = [1 2 3 4] (returns new vec)
vec::push::i64(v, 5)               // Typed variant
vec::pop(v)                        // Removes last, returns it
vec::pop::i64(v)
vec::get(v, 0)                     // Returns value at index
vec::get::i64(v, 0)
vec::set(v, 0, 10)                 // v = [10 2 3] (returns new vec)
vec::set::i64(v, 0, 10)
vec::len(v)                        // Length
vec::len::i64(v)
vec::is_empty(v)                   // Bool
vec::contains(v, 2)                // Bool
vec::contains::i64(v, 2)
vec::slice(v, 1, 3)                // Sub-vector [1..3]
vec::filter::i64(v, (x) => x > 2)  // Filter
vec::map::i64_i64(v, (x) => x * 2) // Map
vec::fold::i64(v, 0, (a b) => a + b) // Reduce
vec::find::i64(v, (x) => x > 2)    // First matching
vec::reverse(v)                    // Reverse in place
vec::unique(v)                     // Remove duplicates
vec::concat(v1, v2)                // Concatenate
```

##// Method Syntax (Sugar)

```mire
set v = [1 2 3] :vec[i64] mut
v.push(4)              // vec::push(v, 4)
v.get(0)               // vec::get(v, 0)
v.len()                // vec::len(v)
v.contains(2)          // vec::contains(v, 2)
v.filter((x) => x > 2) // vec::filter(v, ...)
```

- Parser normalizes `v.method()` -> `vec::method(v)`
- Overloads resolved by receiver type

## Maps (`map[K V]`)

Hash maps, heap-allocated.

```mire
// Creation
set m = {} :map[str i64] mut
set m = {"a": 1, "b": 2} :map[str i64]

// Operations
map::set(m, "c", 3)           // Returns new map
map::set::str(m, "c", 3)
map::get(m, "a")              // Returns value or panics
map::get::str(m, "a")
map::get_or(m, "z", 0)        // With default
map::has(m, "a")              // Bool
map::has::str(m, "a")
map::remove(m, "a")           // Removes key, returns unit
map::keys(m)                  // vec[str]
map::values(m)                // vec[i64]
map::values::i64(m)
map::entries(m)               // vec[tuple[str i64]]
map::len(m)                   // Size
map::is_empty(m)              // Bool
map::merge(m1, m2)            // Merge (m2 wins conflicts)
map::clear(m)                 // Empty map
```

##// Method Syntax

```mire
set m = {"a": 1} :map[str i64] mut
m.set("b", 2)           // map::set(m, "b", 2)
m.get("a")              // map::get(m, "a")
m.has("a")              // map::has(m, "a")
m.len()                 // map::len(m)
```

## Arrays (`arr[T N]`)

Fixed-size, stack-allocated, compile-time length.

```mire
set arr = [1 2 3 4 5] :arr[i64 5]

// Access
set x = arr at 2        // Index (bounds checked)
set x = arr[2]          // Same

// Properties
arr.len()               // Returns N (compile-time constant)
// Note: len() on array returns 1 (size of array as value), not N
```

- Length part of type: `arr[i64 5]` != `arr[i64 4]`
- Passed by value (copied)
- Bounds checking at runtime

## Boxes (`Box[T]`)

Heap-allocated single value, for recursive types.

```mire
set b = box(42) :Box[i64]
set x = *b              // Dereference

// Recursive type example
enum List[T] {
    Nil
    Cons(head :T, tail :Box[List[T]])
}

set list = List::Cons(1, box(List::Cons(2, box(List::Nil))))
```

- `box(value)` allocates on heap
- `*box` dereferences
- Used for recursive data structures

## Common Patterns

##// Iteration

```mire
for item in vec {
    dasu(str::from_i64(item))
}

for item, idx in vec {
    dasu("{idx}: {item}")
}
```

##// Chaining

```mire
set result = vec
    .filter((x) => x > 0)
    .map((x) => x * 2)
    .fold(0, (a b) => a + b)
```

##// Performance Notes

| Operation | Vec | Map | Array |
|-----------|-----|-----|-------|
| Push/Insert | Amortized O(1) | O(1) avg | N/A |
| Get/Index | O(1) | O(1) avg | O(1) |
| Remove | O(n) | O(1) avg | N/A |
| Iterate | O(n) | O(n) | O(n) |
| Memory | Contiguous | Hash table | Stack |

## Type Parameters

```mire
// Vec of vectors
set matrix = [] :vec[vec[i64]] mut

// Map with vec values
set groups = {} :map[str vec[i64]] mut

// Nested generics
set complex = Box[vec[map[str i64]]](value: [])
```

- All collections support nested generics
- Type inference works in most cases
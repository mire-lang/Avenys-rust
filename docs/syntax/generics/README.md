// Generics

## Function Generics

```mire
pub fn identity[T]: (x :T) :T {
    return x
}

pub fn pair[A B]: (a :A, b :B) :tuple[A B] {
    return (a, b)
}

// Explicit instantiation
set a = identity[i64](42)
set b = identity("hello")  // Inferred: T = str

// Multiple params
set p = pair[i64 str](10, "ten")
```

- `[T]` after name declares type parameters
- Explicit args: `fn[T](...)` or inferred from call
- Multiple: `[T U V]`

## Struct Generics

```mire
pub struct Box[T] {
    value :T
}

pub struct Pair[A B] {
    first :A,
    second :B
}

// Usage
set box = Box[i64](value: 42)
set pair = Pair[i64 str](first: 1, second: "one")
```

- `[T]` in struct declaration
- Fields can use type parameters
- Instantiation: `Type[args](fields)`

## Enum Generics

```mire
pub enum Option[T] {
    None
    Some(value :T)
}

pub enum Result[T E] {
    Ok(value :T)
    Err(error :E)
}

// Usage
set opt = Option::Some[i64](42)
set res = Result::Ok[i64 str](42)
```

- `[T]` in enum declaration
- Variants can use type parameters
- Instantiation: `Enum[args]::Variant(...)`

## Impl Blocks for Generics

```mire
impl[T] Box[T] {
    pub fn new: (value :T) :Box[T] {
        return Box(value: value)
    }
    
    pub fn get: (self) :T {
        return self.value
    }
}

impl[T U] Pair[T U] {
    pub fn swap: (self) :Pair[U T] {
        return Pair(first: self.second, second: self.first)
    }
}
```

- `impl[T] Type[T] { ... }`
- Methods can use type parameters
- Self type is `Type[T]`

## Trait Bounds

```mire
// Function bounds
pub fn print_it[T: Printable](x :T) {
    dasu(x.to_string())
}

pub fn sort[T: Comparable](items :vec[T]) {
    // ...
}

// Multiple bounds
pub fn process[T: Printable + Comparable](x :T) {
    dasu(x.to_string())
    // ...
}

// Impl bounds
impl[T: Printable] Container for Vec[T] {
    ...
}
```

- `T: Skill` after type parameter
- Multiple: `T: Skill1 + Skill2`
- In functions, impl blocks, struct/enum declarations

## Where Clauses (Complex Bounds)

```mire
pub fn complex[T U]
    where T: Printable,
          U: Comparable,
          T: Clone
    (t :T, u :U) :T {
    // ...
}
```

- `where` after generic params
- Multiple bounds separated by commas
- Useful for many bounds or complex constraints

## Default Type Parameters

```mire
pub struct Config[T = str] {
    value :T
}

set c1 = Config(value: "default")  // T = str (default)
set c2 = Config[i64](value: 42)    // T = i64
```

- `T = DefaultType` in declaration
- Used when not explicitly provided

## Type Parameter Constraints

```mire
// Sized bound (implicit for most types)
pub fn takes_sized[T: Sized](x :T) { ... }

// Clone bound
pub fn clone_it[T: Clone](x :T) :T {
    return x.clone()
}

// Multiple with defaults
pub fn flexible[T: Printable + Clone = str](x :T) :str {
    return x.to_string()
}
```

## Generic Method Calls

```mire
set box = Box[i64](value: 42)
set val = box.get()  // Returns i64

set pair = Pair[i64 str](first: 1, second: "one")
set swapped = pair.swap()  // Returns Pair[str i64]
```

- Methods on generic types preserve type parameters
- Return types can rearrange parameters

## Limitations

1. **No specialization** - single impl per type
2. **No const generics** - only type parameters
3. **No higher-kinded types** - `F[T]` where `F` is generic
4. **No GATs** - generic associated types
5. **Monomorphization only** - code generated per concrete type
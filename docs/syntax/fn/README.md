// Functions

## Declaration

```mire
// Public function
pub fn add: (a :i64, b :i64) :i64 {
    return a + b
}

// Private function (default)
fn helper: (x :i64) :i64 {
    return x * 2
}

// No return type (unit)
pub fn greet: () {
    dasu("Hello")
}
```

- `pub fn` = public (exported)
- `fn` = private (module-internal)
- Return type after `:`, `()` or omitted for unit
- Parameters: `name :type` separated by spaces (commas tolerated)

## Calls

```mire
set result = add(5 3)      // Idiomatic: space-separated
set result = add(5, 3)     // Commas also accepted
```

- Arguments separated by spaces (commas parsed but not required)
- No parentheses for zero-arg: `fn_name()` not `fn_name`

## Named Arguments

```mire
pub fn greet: (name :str, age :i64) {
    dasu("Hello {name}, age {age}")
}

greet(name: "Alice" age: 30)   // Order independent
greet(age: 30 name: "Bob")
```

- `param_name: value` syntax
- Can be in any order
- All arguments must be named if any are

## Return

```mire
pub fn max: (a :i64, b :i64) :i64 {
    if a > b {
        return a
    }
    return b
}

// Implicit return (last expression)
pub fn add: (a :i64, b :i64) :i64 {
    a + b  // No return needed
}
```

- `return expr` for early exit
- Last expression in block is implicit return (if matches return type)
- Unit functions: `return` or implicit fall-through

## Methods (Instance Functions)

```mire
struct Point { x :i64, y :i64 }

impl Point {
    pub fn dist: (self) :f64 {
        return sqrt((self.x * self.x + self.y * self.y) :f64)
    }
    
    pub fn translate: (self dx :i64 dy :i64) {
        set self.x = self.x + dx
        set self.y = self.y + dy
    }
}
```

- Inside `impl Type { ... }`
- First parameter is `self` (owned), `&self`, or `&mut self`
- Called as `point.dist()`, `point.translate(1 2)`

## Static / Associated Functions

```mire
impl Point {
    pub fn new: (x :i64, y :i64) :Point {
        return Point(x, y)
    }
    
    pub fn origin: () :Point {
        return Point(0, 0)
    }
}

set p = Point::new(3, 4)
set o = Point::origin()
```

- No `self` parameter
- Called via `Type::name(...)`
- Used for constructors, factories

## Nested Functions

```mire
pub fn outer: (x :i64) :i64 {
    pub fn inner: (y :i64) :i64 {
        return x + y  // Captures x from outer scope
    }
    return inner(10)
}
```

- Functions can be defined inside other functions
- **Flattening pass** promotes to top-level as `outer::inner`
- Visibility (`pub`/`fn`) preserved
- Captured variables become implicit parameters

## Function Pointers / First-Class Functions

```mire
set f = add  // Function value (type: fn(i64 i64) :i64)

pub fn apply: (f fn(i64 i64) :i64, a :i64, b :i64) :i64 {
    return f(a b)  // Call via function value
}
```

- Function names usable as values
- Type: `fn(param_types) :return_type`
- Passed to higher-order functions (`vec::map`, etc.)

## Generic Functions

```mire
pub fn identity[T]: (x :T) :T {
    return x
}

set a = identity[i64](42)      // Explicit instantiation
set b = identity("hello")      // Inferred: T = str
```

- `[T]` after name declares type parameter
- Explicit args: `fn[T](...)` or inferred from call
- Multiple params: `[T U]`
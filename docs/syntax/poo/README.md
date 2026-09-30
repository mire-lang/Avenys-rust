// Object-Oriented Programming

## Structs

```mire
pub struct Point {
    x :i64,
    y :i64
}

pub struct Person {
    name :str,
    age :i32,
    email :str
}
```

- `pub struct` = public (exported)
- `struct` = private (module-internal)
- Fields: `name :type` separated by newlines or spaces
- No commas in field list

## Construction

```mire
// Named fields (order independent)
set p = Point(x: 3, y: 4)

// Positional (field order must match)
set p = Point(3, 4)

// With type ascription
set p = Point(x: 3, y: 4) :Point
```

- Named: `Type(field: value, ...)`
- Positional: `Type(value, ...)` (order = declaration order)
- Both forms valid

## Methods (Instance Functions)

```mire
impl Point {
    pub fn dist: (self) :f64 {
        return sqrt((self.x * self.x + self.y * self.y) :f64)
    }
    
    pub fn translate: (self dx :i64 dy :i64) {
        set self.x = self.x + dx
        set self.y = self.y + dy
    }
}

set p = Point(3, 4)
set d = p.dist()
p.translate(1, 2)
```

- Inside `impl Type { ... }`
- First param: `self` (owned), `&self`, or `&mut self`
- Called with dot syntax: `obj.method(args)`
- `self` mutable allows field mutation

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

## Struct Inheritance (extends)

```mire
pub struct Animal {
    name :str
}

pub struct Dog extends Animal {
    breed :str
}

impl Dog {
    pub fn greet: (self) :str {
        return self.name + " the " + self.breed
    }
}

set dog = Dog(name: "Rex" breed: "Labrador")
dasu(dog.greet())  // "Rex the Labrador"
```

- `extends Parent` for single inheritance
- Child inherits **all parent fields**
- Constructor accepts parent + child fields
- Parent fields accessible via `self` in child impl
- **Single inheritance only** (no multiple)

## Field Access

```mire
set p = Point(3, 4)

// Read
set x = p.x
set y = p.y

// Write (requires mut on struct)
set p = Point(3, 4) mut
set p.x = 10
set p.y = 20

// Via reference
set ref = &p
set x = ref.x  // Auto-deref
```

- Dot syntax: `obj.field`
- Auto-deref on `&T` / `&mut T`
- Write requires mutable binding

## Visibility

```mire
pub struct PublicStruct {  // Exported
    pub field: i64         // Exported field
    private_field: i64     // Module-internal
}

struct PrivateStruct {     // Module-internal
    ...
}
```

- `pub` on struct = exported
- `pub` on field = exported
- No `pub` = module-private

## Limitations

1. **Single inheritance only** - no multiple parents
2. **No method overriding** - child cannot override parent methods
3. **No private/protected fields** - only `pub` or private
4. **No virtual methods** - all dispatch static
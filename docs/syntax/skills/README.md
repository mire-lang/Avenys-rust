// Skills (Traits/Interfaces)

## Declaration

```mire
// Simple skill
pub skill Printable {
    fn to_string: (self) :str
}

// Skill with multiple methods
pub skill Comparable {
    fn equals: (self other :&Self) :bool
    fn less_than: (self other :&Self) :bool
}

// Generic skill
pub skill Container[T] {
    fn len: (self) :i64
    fn get: (self index :i64) :T
}
```

- `pub skill` = exported
- Methods declared with `fn name: (params) :return`
- `Self` refers to implementing type
- Generic parameters: `skill Name[T] { ... }`

## Implementation

```mire
impl Printable for Point {
    fn to_string: (self) :str {
        return "Point({self.x}, {self.y})"
    }
}

impl Comparable for i64 {
    fn equals: (self other :&i64) :bool {
        return self == other
    }
    
    fn less_than: (self other :&i64) :bool {
        return self < other
    }
}
```

- `impl Skill for Type { ... }`
- Must provide all methods declared in skill
- `Self` in skill becomes implementing type

## Skill Inheritance (super)

```mire
pub skill Describable {
    fn describe: (self) :str
}

pub skill Detailed super Describable {
    fn detail: (self) :str
}

// Implementing Detailed requires BOTH methods
impl Detailed for Point {
    fn describe: (self) :str {
        return "Point at ({self.x}, {self.y})"
    }
    
    fn detail: (self) :str {
        return "x={self.x}, y={self.y}, dist={self.dist()}"
    }
}
```

- `super ParentSkill` extends parent
- Child skill inherits parent's method signatures
- Implementing child requires all parent + child methods
- **Single inheritance only**

## Usage

```mire
// As bound
pub fn print_it[T: Printable](x :T) {
    dasu(x.to_string())
}

// With multiple bounds
pub fn sort[T: Comparable + Printable](items :vec[T]) {
    // ...
}

// In impl bounds
impl[T: Printable] Container for Vec[T] {
    ...
}
```

- `T: Skill` in function generics
- Multiple bounds: `T: Skill1 + Skill2`
- In impl headers: `impl[T: Skill] Skill for Type`

## Dispatch

```mire
set p = Point(3, 4)
p.to_string()        // Static dispatch (known at compile time)
print_it(p)          // Monomorphized per concrete type

// Dynamic dispatch NOT supported
// No trait objects (dyn Trait)
```

- **Static dispatch only** - monomorphization
- No `dyn Trait`, no vtables
- Each concrete type gets specialized code

## Skills vs Struct Inheritance

| Aspect | Skills | Struct Inheritance |
|--------|--------|-------------------|
| Purpose | Behavior contracts | Data reuse |
| Methods | Required signatures | Optional (child adds) |
| Data | None | Inherits fields |
| Multiple | No | No |
| Dispatch | Static (monomorphized) | Static |

## Limitations

1. **No dynamic dispatch** - no `dyn Skill`
2. **No default implementations** - all methods required
3. **Single inheritance** - one `super` only
4. **No associated types** - use generics instead
5. **No async methods in skills** - use explicit async patterns
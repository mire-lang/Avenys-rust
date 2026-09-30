// Variables

## Declaration vs Reassignment

The `set` keyword handles both declaration and reassignment via scope lookup:

```mire
// Declaration (new binding)
set name = "Mire"

// Reassignment (existing binding in current or parent scope)
set name = "Avenys"
```

- First `set` in a scope creates a new binding
- Subsequent `set` on same name reassigns (lookup walks parent scopes)
- Shadowing: `set name = ...` in inner scope creates new binding

## Immutability by Default

```mire
set name = "Mire"        // Immutable binding
// set name = "Other"     // Error: cannot reassign immutable
```

- All bindings are immutable unless explicitly marked `mut`

## Constants

```mire
set PI = 3.14159 :f64 const
// set PI = 2.71        // Error: cannot reassign const
```

- `const` modifier makes binding compile-time constant
- Value must be known at compile time
- Strict prohibition on any reassignment

## Explicit Mutability

```mire
set counter = 0 :i64 mut

pub fn increment: () {
    set counter = counter + 1  // OK: mutable
}
```

- `:type mut` annotation required for mutable bindings
- Type ascription (`:i64`) optional but recommended
- Mutable bindings can be reassigned freely

## Compound Assignment

```mire
set x = 10 :i64 mut
set x += 5    // x = 15
set x -= 3    // x = 12
```

- Operators: `+=`, `-=`
- Only available on mutable bindings
- Equivalent to `set x = x + 5` but more concise

## Scope Rules

```mire
pub fn outer: () {
    set x = 1 :i64 mut
    
    pub fn inner: () {
        set x = 2      // Reassigns outer x (no local x)
        set y = 3 :i64 // New local binding
    }
    
    inner()
    // x is now 2
}
```

- `set` looks up through parent scopes for existing binding
- Creates new binding only if none found in any parent scope
- Use `mut` on original declaration to allow reassignment from inner scopes
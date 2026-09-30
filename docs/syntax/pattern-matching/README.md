// Pattern Matching

## match Expression

```mire
set result = match value {
    1 => "one"
    2 => "two"
    3 | 4 | 5 => "three to five"
    x when x > 10 => "large: {x}"
    _ => "other"
} :str
```

- Evaluates to a value (expression form)
- All arms must produce same type
- Type ascription after `}` required if not inferrable

## Patterns

##// Literal Patterns

```mire
match x {
    0 => "zero"
    42 => "answer"
    "hello" => "greeting"
    true => "yes"
}
```

- Integers, floats, strings, booleans, chars
- Exact value match

##// Variable Binding

```mire
match x {
    y => "caught: {y}"  // Binds any value to y
}
```

- Binds matched value to new variable
- Variable scope limited to arm

##// Wildcard

```mire
match x {
    1 => "one"
    _ => "everything else"  // Must be last
}
```

- `_` matches any value
- No binding
- Conventionally placed last

##// Or-Patterns (Alternatives)

```mire
match x {
    1 | 2 | 3 => "small"
    10 | 20 | 30 => "round tens"
    "a" | "b" | "c" => "early letter"
}
```

- Multiple patterns separated by `|`
- All share same arm body
- All patterns must bind same variables (or none)

##// Range Patterns

```mire
match x {
    0..10 => "single digit"
    10..100 => "double digit"
    _ => "large"
}
```

- Inclusive range: `start..end`
- Works on integers, chars
- Cannot overlap with other patterns

##// Guard Patterns (when)

```mire
match x {
    n when n % 2 == 0 => "even: {n}"
    n when n > 100 => "big: {n}"
    _ => "other"
}
```

- `pattern when condition`
- Condition can use bound variables
- Evaluated after pattern matches

##// Enum Variant Patterns

```mire
enum Option[T] { None, Some(value :T) }

match opt {
    None => "empty"
    Some(v) => "value: {v}"
}
```

- Matches enum variants
- Payload extracted via binding: `Some(v)`
- Exhaustiveness checked for enums

##// Struct Patterns

```mire
struct Point { x :i64, y :i64 }

match p {
    Point(x: 0, y: 0) => "origin"
    Point(x: x, y: 0) => "on x-axis: {x}"
    Point(x: _, y: y) => "y = {y}"
}
```

- Matches struct fields by name
- `_` for ignored fields
- Can bind fields to variables

##// Ref Patterns

```mire
match &value {
    &x => "got reference to {x}"
}
```

- `&pattern` matches through reference
- Dereferences automatically

## Match Statement (No Return)

```mire
match value {
    1 { dasu("one") }
    2 { dasu("two") }
    _ { dasu("other") }
}
```

- No `=>`, uses braces for body
- No return value
- Used for side effects

## Exhaustiveness

- Compiler verifies all cases covered for enums
- Non-exhaustive match on enum = error
- Wildcard `_` makes any match exhaustive
- Integers/strings: exhaustiveness not enforced (infinite domain)
// Control Flow

## Conditionals

##// if / else

```mire
if condition {
    // then branch
} else {
    // else branch
}
```

- `else` **must be on same line** as closing brace: `} else {`
- Braces required (no brace-less single-statement form)

##// if as Expression

```mire
set max = if a > b { a } else { b }
```

- `if`/`else` can be used as expression
- Both branches must produce same type
- Type of expression is common type of branches

## Loops

##// while

```mire
set i = 0 :i64 mut
while i < 10 {
    dasu(str::from_i64(i))
    set i += 1
}
```

- Condition evaluated before each iteration
- Body executes while condition is true

##// do / while

```mire
set i = 0 :i64 mut
do {
    dasu(str::from_i64(i))
    set i += 1
} while i < 10
```

- Body executes **at least once**
- Condition checked after each iteration

##// for (Iteration)

```mire
// Over range
for i in range(0 10) {
    dasu(str::from_i64(i))
}

// Over vector
for item in vec {
    dasu(str::from_i64(item))
}

// With index
for item, idx in vec {
    dasu("index {idx}: {item}")
}
```

- `range(start end)` produces sequence `[start, end)`
- `range(start end step)` with custom step
- Iterates over vectors, arrays, ranges
- Optional index variable: `for item, idx in collection`

##// find

```mire
find item in collection {
    if item > 10 {
        return item  // Early exit with value
    }
} else {
    dasu("Not found")
}
```

- Searches for first element matching condition
- `return` inside exits with value
- `else` branch executes if no match found

## Loop Control

```mire
while true {
    set x = read_input()
    if x == 0 {
        break      // Exit loop immediately
    }
    if x < 0 {
        continue   // Skip to next iteration
    }
    process(x)
}
```

- `break` exits innermost loop
- `continue` skips to next iteration
- Work in `while`, `do/while`, `for`, `find`

## Match Expressions

```mire
set result = match value {
    1 => "one"
    2 => "two"
    3 | 4 | 5 => "three to five"
    x when x > 5 => "large: {x}"
    _ => "other"
} :str
```

- Pattern matching on values
- `=>` separates pattern from result expression
- Multiple patterns: `p1 | p2 | p3`
- Guards: `pattern when condition`
- Wildcard `_` matches anything (must be last)
- Returns value (expression form)

## Match Statements

```mire
match value {
    1 { dasu("one") }
    2 { dasu("two") }
    _ { dasu("other") }
}
```

- Statement form (no return value)
- Braces for each case body
- Used for side effects

## Break / Continue in Match

```mire
while true {
    match read_input() {
        0 { break }      // Exit loop
        -1 { continue }  // Next iteration
        x { process(x) }
    }
}
```

- `break`/`continue` allowed in match arms inside loops
- Affects innermost enclosing loop
// Operators

## Arithmetic

| Operator | Description | Types |
|----------|-------------|-------|
| `+` | Addition | integers, floats |
| `-` | Subtraction | integers, floats |
| `*` | Multiplication | integers, floats |
| `/` | Division | integers (inline, div-by-zero check), floats |
| `%` | Remainder | integers (inline, div-by-zero check) |
| `**` | Power | floats only (`math::pow`) |

##// Integer Division

```mire
set a = 10 / 3   // 3 (inline sdiv with zero check)
set b = 10 % 3   // 1 (inline srem with zero check)
// Division by zero -> panic via rt_panic_loc
```

## Float Division

```mire
set a = 10.0 / 3.0   // 3.333... (native fdiv)
set b = 10.0 % 3.0   // 1.0 (native frem, IEEE 754)
```

## Comparison

| Operator | Description |
|----------|-------------|
| `==` | Equal |
| `!=` | Not equal |
| `<` | Less than |
| `<=` | Less or equal |
| `>` | Greater than |
| `>=` | Greater or equal |

```mire
if a == b { ... }
if x < 10 { ... }
```

- Result type: `bool`
- Works on integers, floats, strings, bools
- Strings: lexicographic comparison

## Boolean

| Operator | Description |
|----------|-------------|
| `&&` | Logical AND |
| `\|\|` | Logical OR |
| `!` | Logical NOT |

```mire
if a && b { ... }
if x > 0 || y > 0 { ... }
if !flag { ... }
```

- Short-circuit evaluation
- Operands must be `bool`

## Bitwise

| Operator | Description |
|----------|-------------|
| `&` | Bitwise AND |
| `\|` | Bitwise OR |
| `^` | Bitwise XOR |
| `<<` | Left shift |
| `>>` | Right shift (arithmetic) |

```mire
set flags = 0b1010 & 0b1100  // 0b1000 (8)
set x = 1 << 3               // 8
set y = 16 >> 2              // 4
```

- Works on integer types only
- `&` is **bitwise AND**, not reference (see disambiguation below)

## Assignment

| Operator | Description |
|----------|-------------|
| `=` | Assignment (or declaration) |
| `+=` | Add and assign |
| `-=` | Subtract and assign |

```mire
set x = 10 :i64 mut
set x += 5   // x = 15
set x -= 3   // x = 12
```

- Only on mutable bindings
- Equivalent to `set x = x + 5`

## Pipeline

| Operator | Description |
|----------|-------------|
| `=>` | Standard pipeline |
| `?=>` | Safe pipeline (error propagation) |

```mire
set result = 5 => add(3) => mul(2)
set result = maybe_val ?=> process
```

- Lowest precedence
- See [Pipeline documentation](./pipeline.md)

## Precedence (Highest to Lowest)

1. **Postfix**: `()`, `.`, `[]`, `++`, `--` (not in Mire)
2. **Unary**: `!`, `-`, `&`, `*`, `box`
3. **Multiplicative**: `*`, `/`, `%`
4. **Additive**: `+`, `-`
5. **Shift**: `<<`, `>>`
6. **Bitwise AND**: `&`
7. **Bitwise XOR**: `^`
8. **Bitwise OR**: `|`
9. **Comparison**: `<`, `<=`, `>`, `>=`
10. **Equality**: `==`, `!=`
11. **Logical AND**: `&&`
12. **Logical OR**: `\|\|`
13. **Pipeline**: `=>`, `?=>`
14. **Assignment**: `=`, `+=`, `-=`

## Disambiguation: `&` Operator vs Reference

```mire
// Bitwise AND (operands are integers)
set result = a & b

// Reference (operand is variable, prefix position)
set ref = &x
set mut_ref = &mut x
```

**Rule**: `&` is bitwise AND in infix position (between two operands). It is reference operator in prefix position (before a single operand).

```mire
// These are bitwise AND:
a & b
x & y & z
(a + b) & c

// These are references:
&x
&mut x
&(x + y)  // Parentheses make it prefix
```

## Multiline Expressions

```mire
set result = a + b
    + c
    + d
```

- Operator at **end of line** continues expression
- Works for `+`, `-`, `*`, `/`, `%`, `&&`, `\|\|`, `&`, `|`, `^`, `<<`, `>>`
- Not needed for pipeline (`=>`) which already chains
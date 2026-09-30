// Strings

## String Type

```mire
set s = "hello" :str
// Internally: fat pointer { ptr: *u8, len: usize }
// UTF-8 encoded, NUL-terminated for C interop
```

- Immutable by default
- Length-prefixed (not NUL-terminated for Mire operations)
- NUL-terminated for C FFI

## Native Module (`strings::*`)

```mire
load mire::str

// Case
strings::upper(s)       // "HELLO"
strings::lower(s)       // "hello"

// Trimming
strings::trim(s)        // Remove leading/trailing whitespace
strings::strip(s)       // Alias for trim

// Search
strings::contains(s, "ell")        // true
strings::starts::with(s, "he")     // true
strings::ends::with(s, "lo")       // true
strings::index(s, "ell")           // 1 (position)
strings::index(s, "xyz")           // -1 (not found)

// Replacement
strings::replace(s, "l", "L")      // "heLLo" (all)
strings::replace::first(s, "l", "L") // "heLlo" (first only)

// Splitting/Joining
strings::split(s, " ")             // vec[str]
strings::join(parts, " ")          // str

// Substring
strings::substr(s, 1, 3)           // "ell" (start, len)
strings::substr(s, 1, -1)          // "ello" (to end)

// Repetition
strings::repeat(s, 3)              // "hellohellohello"

// Padding
strings::pad::left(s, 10, "-")     // "-----hello"
strings::pad::right(s, 10, "-")    // "hello-----"

// Character access
strings::char_at(s, 1)             // 'e' (as i64 codepoint)

// Conversions
strings::from::i64(42)             // "42"
strings::from::f64(3.14)           // "3.14"
strings::from::bool(true)          // "true"
strings::to_i64("42")              // 42
strings::to_f64("3.14")            // 3.14

// Length
strings::len(s)                    // 5

// Low-level
strings::concat(s1, s2)            // s1 + s2
strings::copy(s)                   // Deep copy
```

## Method Syntax (Sugar)

```mire
load mire::str

set s = "hello"

s.upper()              // strings::upper(s)
s.lower()
s.trim()
s.strip()
s.contains("ell")
s.starts::with("he")
s.ends::with("lo")
s.index("ell")
s.replace("l", "L")
s.replace::first("l", "L")
s.split(" ")
s.join(" ")
s.substr(1, 3)
s.repeat(3)
s.pad::left(10, "-")
s.pad::right(10, "-")
s.char_at(1)
s.len()
s + " world"           // strings::concat(s, " world")
```

- Parser normalizes `s.method()` -> `strings::method(s)`
- Nested namespaces: `s.starts::with("he")` -> `strings::starts::with(s, "he")`
- Overloads resolved by receiver type

## Concatenation

```mire
set a = "hello"
set b = "world"
set c = a + " " + b      // "hello world"
set c = strings::concat(a, " ", b)
```

- `+` operator for string concatenation
- Chained: `a + b + c` -> single `rt_string_concat_n` call
- Compile-time constant folding: `"a" + "b"` -> `"ab"`

## Interpolation (dasu)

```mire
set name = "Alice"
set age = 30

dasu("Hello {name}, you are {age}")  // Prints: Hello Alice, you are 30
```

- `dasu(msg)` prints with interpolation
- `{var}` syntax for variables
- `{expr}` not supported (use variable)

## Literals

```mire
"hello"              // Regular string
"line1\nline2"       // Escape sequences
"tab\there"          // \t, \n, \r, \\, \", \'
"unicode: \u{1F600}" // Unicode escape (hex codepoint)
r"raw\nstring"       // Raw string (no escapes)
r#"raw with "quotes"//  // Raw with custom delimiter
```

##// Escape Sequences

| Sequence | Meaning |
|----------|---------|
| `\n` | Newline |
| `\r` | Carriage return |
| `\t` | Tab |
| `\\` | Backslash |
| `\"` | Double quote |
| `\'` | Single quote |
| `\u{XXXX}` | Unicode codepoint (hex) |

## Conversions

```mire
// To string
strings::from::i64(42)     // "42"
strings::from::u64(42)     // "42"
strings::from::f64(3.14)   // "3.14"
strings::from::bool(true)  // "true"

// From string
strings::to_i64("42")      // 42
strings::to_u64("42")      // 42
strings::to_f64("3.14")    // 3.14
strings::to_bool("true")   // true
```

## Length and Indexing

```mire
set s = "hello"

strings::len(s)        // 5 (byte length = codepoint length for ASCII)
strings::char_at(s, 1) // 101 ('e' as i64)

// For Unicode: len = byte count, char_at = codepoint
set u = "héllo"        // 6 bytes, 5 codepoints
strings::len(u)        // 6
strings::char_at(u, 1) // 233 ('é')
```

## C FFI

```mire
// To C string (NUL-terminated)
extern fn puts: (msg :*mut i8) :i32 lib "c"
puts(strings::to_cstr(s))  // Requires manual C-string conversion

// From C string
extern fn getenv: (name :*const i8) :*mut i8 lib "c"
set s = strings::from_cstr(getenv("PATH"))
```

- Mire `str` is fat pointer, C expects `char*`
- Conversion helpers in `strings::` module

## Performance

| Operation | Complexity |
|-----------|------------|
| `len()` | O(1) |
| `concat` / `+` | O(n) (new allocation) |
| `substr` | O(n) (new allocation) |
| `replace` | O(n) |
| `split` | O(n) |
| `upper/lower` | O(n) |

- Strings are immutable (new allocation on modification)
- Use `vec[str]` builder pattern for many concatenations
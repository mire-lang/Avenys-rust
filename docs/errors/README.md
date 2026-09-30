# Error Code Reference

> All compiler error and warning codes with descriptions and fixes.

Version: **4.1.0**
Status: **Stable**
Date: **2026-09-12**

---

## Overview

Avenys uses a structured error code system. Each error has a unique code, a descriptive message, and guidance on how to fix it.

---

## Error Codes (`E0xxx`)

### E0001 - Lexer: Unexpected Token

```
error[E0001] Lexical Error
  at source-file:line:col
  unexpected token `...`
  Expected a valid token.
```

**Fix**: Check the token sequence near the error location. Common causes: missing operator, misplaced punctuation, invalid character.

---

### E0003 - Parser: Syntax Error

```
error[E0003] Syntax Error
  at source-file:line:col
  unexpected `}`
  Expected statement or block terminator.
```

**Fix**: Verify braces are balanced. Ensure `} else {` is on the same line. Check for missing `fn`, `struct`, `enum`, `impl`, or `module` keywords.

---

### E0005 - Type: Type Mismatch

```
error[E0005] Type Error
  at source-file:line:col
  expected i64, found str
  review the declared type and assigned expression
```

**Fix**: Add explicit type ascription (`:i64`) or ensure the expression type matches. Check function return types.

---

### E0007 - Ownership: Use After Move

```
error[E0007] Ownership Error
  at source-file:line:col
  use of moved value
  Borrow the value instead of moving it, or clone it.
```

**Fix**: Use `&value` to borrow instead of moving. Or clone the value if ownership transfer is needed.

---

### E0008 - Type: Cannot Infer Type

```
error[E0008] Type Error
  at source-file:line:col
  cannot infer type for `x`
  Add an explicit type annotation: `set x = ... :i64`
```

**Fix**: Add explicit type ascription. The compiler cannot determine the type from context alone.

---

### E0009 - Parser: Expected Expression

```
error[E0009] Syntax Error
  at source-file:line:col
  expected expression
  Provide a valid expression.
```

**Fix**: Check for missing operands, incomplete function calls, or misplaced operators.

---

### E0010 - Type: Type Mismatch

```
error[E0010] Type Error
  at source-file:line:col
  type mismatch
  Ensure types align or add explicit ascription.
```

**Fix**: Verify type compatibility. Use explicit type annotations where inference fails.

---

### E0011 - Type: Invalid Type

```
error[E0011] Type Error
  at source-file:line:col
  invalid type `...`
  Check the type name and ensure it is defined.
```

**Fix**: Verify the type exists and is in scope. Check spelling and `load` statements.

---

### E0013 - Parser: Unterminated Block

```
error[E0013] Syntax Error
  at source-file:line:col
  unterminated block
  Add the missing `}`.
```

**Fix**: Add the missing closing brace. Common in `if`, `while`, `fn`, `impl`, and `match` blocks.

---

### E0014 - Backend: Backend Limitation

```
error[E0014] Backend Limitation
  at source-file:line:col
  cannot lower this construct
  The frontend accepted this program, but the backend cannot lower it yet.
```

**Fix**: Refactor the code to use supported constructs. Check the Avenys issue tracker for upcoming support.

---

### E0015 - Runtime: Runtime Error

```
error[E0015] Runtime Error
  at source-file:line:col
  runtime error occurred
  Check the runtime message for details.
```

**Fix**: The error occurred at runtime. Check input values, file paths, and resource availability.

---

### E0016 - Ownership: Borrow Violation

```
error[E0016] Ownership Error
  at source-file:line:col
  borrow violation
  Ensure no mutable borrows exist while shared borrows are active.
```

**Fix**: Reorder operations or clone values to avoid conflicting borrows.

---

### E0017 - CLI: CLI Error

```
error[E0017] CLI Error
  at Invalid command-line arguments
  Check the command syntax and required flags.
```

**Fix**: Run `mire --help` for usage information. Verify all required flags are present.

---

### E0018 - Parser: Ambiguous Syntax

```
error[E0018] Syntax Error
  at source-file:line:col
  ambiguous syntax
  The parser cannot determine the intended construct.
```

**Fix**: Add parentheses or disambiguate the expression.

---

### E0019 - Macro: Macro Not Allowlisted

```
error[E0019] Macro Error
  at macro name not allowlisted
  Add the macro name to `[macros]` in `owl.toml`.
```

**Fix**: Add the macro function name to the `[macros]` section of `owl.toml`.

---

### E0020 - Macro: Invalid Macro Syntax

```
error[E0020] Macro Error
  at invalid macro invocation
  Use `name!(args)` syntax for allowlisted macros.
```

**Fix**: Ensure macro calls use `!` syntax and the macro is declared with `@[macro!]`.

---

### E0021 - Module: Unknown Module

```
error[E0021] Module Error
  at unknown module `...`
  Check the `load` statement and module path.
```

**Fix**: Verify the module exists and is properly declared with `module` or loaded via `load`.

---

### E0022 - Module: Duplicate Definition

```
error[E0022] Module Error
  at duplicate definition of `...`
  Remove or rename the duplicate.
```

**Fix**: Each declaration must be unique within its module. Check for accidental redefinitions.

---

### E0023 - Module: Circular Dependency

```
error[E0023] Module Error
  at circular dependency detected
  Break the cycle by extracting shared code.
```

**Fix**: Refactor to remove circular `load` dependencies between modules.

---

### E0024 - Module: Import Error

```
error[E0024] Module Error
  at import error: `...`
  Check the import path and `load` statement.
```

**Fix**: Verify the module path exists and is accessible.

---

### E0100-E0110 - Type Width/Cast Errors

```
error[E0100] Type Error
  at type width error
  Mire uses real types with exact widths. Use an explicit cast `(value :T)`.
```

These errors relate to integer width mismatches, implicit narrowing, and invalid casts.

**Fix**: Use explicit type ascription for width conversions. `set x = value :i8` for narrowing, `set x = value :i64` for widening.

---

## Warning Codes (`W0xxx`)

### W0001 - Unused Variable

```
warning[W0001] Unused Variable
  at source-file:line:col
  variable `x` is never used
  Remove the variable or use `_` prefix to suppress.
```

### W0002 - Unused Import

```
warning[W0002] Unused Import
  at source-file:line:col
  `module` is imported but never used
  Remove the `load` statement or use the import.
```

### W0004 - Dead Code

```
warning[W0004] Dead Code
  at source-file:line:col
  unreachable code
  Remove the unreachable block.
```

### W0005 - Implicit Type Conversion

```
warning[W0005] Implicit Type Conversion
  at source-file:line:col
  implicit conversion from `i64` to `i8`
  Add explicit cast `(value :i8)`.
```

### W0006 - Performance

```
warning[W0006] Performance
  at source-file:line:col
  inefficient operation
  Consider using a more efficient alternative.
```

### W0007 - Style: Naming Convention

```
warning[W0007] Style
  at source-file:line:col
  naming convention violation
  Use snake_case for variables and functions.
```

### W0008 - Style: Indentation

```
warning[W0008] Style
  at source-file:line:col
  indentation issue
  Use consistent 4-space indentation.
```

### W0009 - Deprecated Function

```
warning[W0009] Deprecated
  at source-file:line:col
  `fn_name` is deprecated
  Use the replacement function.
```

### W0010 - Deprecated Syntax

```
warning[W0010] Deprecated
  at source-file:line:col
  deprecated syntax `...`
  Update to the current syntax.
```

### W0011 - Unused Function

```
warning[W0011] Unused
  at source-file:line:col
  function `fn_name` is never called
  Remove the function or call it.
```

### W0012 - Shadowed Variable

```
warning[W0012] Type
  at source-file:line:col
  variable `x` shadows another `x`
  Rename the inner variable or use a different name.
```

### W0013 - Logic: Unreachable Match Arm

```
warning[W0013] Logic
  at source-file:line:col
  unreachable match arm
  Remove the unhandled pattern or reorder patterns.
```

### W0014 - Complexity

```
warning[W0014] Complexity
  at source-file:line:col
  nested complexity exceeds threshold
  Refactor into smaller functions.
```

### W0017 - Memory: Unreleased Resource

```
warning[W0017] Memory
  at source-file:line:col
  resource may not be released
  Ensure all resources are properly cleaned up.
```

### W0018 - Memory: Potential Leak

```
warning[W0018] Memory
  at source-file:line:col
  potential memory leak
  Verify all allocations are properly freed.
```

### W0019 - Style: Missing Documentation

```
warning[W0019] Style
  at source-file:line:col
  missing documentation
  Add a doc comment to the declaration.
```

### W0021 - Trait Bound Warning

```
warning[W0021] Type
  at source-file:line:col
  trait bound may not be satisfied
  Verify the type implements the required trait.
```

### W0024 - Performance: Inefficient Pattern

```
warning[W0024] Performance
  at source-file:line:col
  inefficient pattern
  Consider a more efficient approach.
```

### W0025 - Module: Circular Dependency

```
warning[W0025] Module
  at source-file:line:col
  circular dependency
  Break the cycle by extracting shared code.
```

### W0034 - Deprecated Function Call

```
warning[W0034] Deprecated
  at source-file:line:col
  call to deprecated function
  Use the replacement function.
```

### W0038 - Duplicate Match Pattern

```
warning[W0038] Logic
  at source-file:line:col
  duplicate literal pattern in match
  Remove the duplicate pattern.
```

---

## Notes

- Warnings are **off by default** - enable with `--show-warn`
- Warnings can be promoted to errors with `-W <code>` or `--warnings-as-errors`
- Error codes are version-stable within a major release
- Runtime errors use `PAL_ERR_*` codes from the PAL layer (see [PAL](../pal/README.md))

---

## Related

- [PAL v4](../pal/README.md)
- [Compiler CLI](../compiler/README.md)
- [Avenys Documentation Index](../README.md)

---

## License

GNU General Public License v3.0

# Error & Warning Codes

## Error Codes (E)

| Code | Title | When it appears | How to fix |
|------|-------|-----------------|------------|
| E0001 | Lexical Error | Invalid characters, unterminated strings/raw strings, invalid integer prefixes, malformed char literals | Check for stray or unsupported characters; ensure strings, char literals, and raw strings are properly terminated |
| E0002 | *(unused)* | Reserved | — |
| E0003 | Syntax Error | Parser encounters unexpected tokens (missing braces, invalid expressions, misplaced keywords) | Review syntax around the marked location; ensure all blocks `{}` are properly opened/closed; check expression structure |
| E0004 | *(unused)* | Reserved | — |
| E0005 | Type Error | Type mismatch in assignments, operator applications, function arguments, or return values | Review the declared types and assigned expressions; ensure operator operand types match |
| E0006 | Identifier Error | Use of undefined or unresolvable identifier | Define the identifier before use; check for typos or missing imports |
| E0007 | Ownership Error | Use-after-move: accessing a value after it has been moved | Reorder operations or clone the value before the move |
| E0008 | Ownership Error | Multiple mutable references to the same value in the same scope | Restructure code to avoid overlapping mutable borrows |
| E0009 | Ownership Error | Mutation while a shared reference is active | Ensure no shared references exist when mutating; consider using a cell type or restructuring |
| E0010 | Ownership Error | Move while value is borrowed, or invalid move operation | Ensure borrowed values are not moved; check move semantics |
| E0011 | Ownership Error | Drop while value has active references | Ensure no references exist before dropping |
| E0012 | Ownership Error | Double drop detected | Review ownership: each value should be dropped exactly once |
| E0013 | Ownership Error | Borrow outlives owner scope, or unsafe block violation | Ensure borrows do not outlive the borrowed value; review unsafe blocks |
| E0014 | Backend Limitation | The frontend accepted the program, but the current backend cannot lower a construct (e.g., tuples, `contains`) | Use an alternative approach or implement the missing lowering |
| E0015 | Runtime Error | I/O errors, file not found, process failures, or other runtime failures during compilation | Check file paths, permissions, and system resources; ensure runtime dependencies are available |

## Warning Codes (W)

### Unused (W0001–W0003)

| Code | Title | Category | When it appears | How to fix |
|------|-------|----------|-----------------|------------|
| W0001 | Unused binding | Unused | A variable/constant is declared but never read | Remove the binding or prefix with `_` |
| W0002 | Unused import | Unused | A module or symbol is imported but never used | Remove the unused import |
| W0003 | Unused function | Unused | A function is defined but never called | Remove or use the function |

### Type (W0004–W0005, W0020–W0021)

| Code | Title | When it appears | How to fix |
|------|-------|-----------------|------------|
| W0004 | Type annotation unnecessary | Redundant explicit type that could be inferred | Remove the annotation |
| W0005 | Suspicious type cast | Cast between incompatible or lossy types | Verify the cast is intentional |
| W0020 | Type coercion loss | Implicit coercion may lose precision | Add explicit conversion |
| W0021 | Type mismatch hint | Operation has mismatched but compatible types | Ensure types are compatible |

### Performance (W0007–W0009, W0027, W0033)

| Code | Title | When it appears | How to fix |
|------|-------|-----------------|------------|
| W0007 | Large function | Function exceeds complexity threshold | Split into smaller functions |
| W0008 | Recursive without memoization | Pure recursive function without caching | Add memoization or iterative approach |
| W0009 | Unnecessary allocation | Heap allocation where stack would suffice | Use stack-allocated alternatives |
| W0027 | Repeated computation | Same expression computed multiple times | Extract to a variable |
| W0033 | Loop invariant | Expression inside loop that doesn't change per iteration | Move outside the loop |

### Style (W0006, W0012–W0014, W0022–W0024, W0026, W0034–W0035, W0037)

| Code | Title | When it appears | How to fix |
|------|-------|-----------------|------------|
| W0006 | Unnecessary `return` | Final expression uses `return` instead of implicit return | Remove `return` keyword |
| W0012 | Naming convention violation | Identifier does not follow `snake_case` convention | Rename to follow conventions |
| W0013 | Long line | Exceeds recommended line length | Break into multiple lines |
| W0014 | Complex expression | Expression too nested or complex | Extract sub-expressions into variables |
| W0022 | Redundant pattern | Match arm pattern that matches the same as a previous arm | Remove redundant arm |
| W0023 | Missing `else` branch | `if` without `else` where both branches expected | Add `else` branch |
| W0024 | Redundant block | Block `{}` around single statement without scoping needs | Remove unnecessary braces |
| W0026 | Comparison to bool | `== true` or `== false` in condition | Use the value directly or negate with `!` |
| W0034 | Inconsistent spacing | Spacing doesn't match project style | Format according to project conventions |
| W0035 | Unnecessary parentheses | Extra parentheses around expression | Remove parentheses |
| W0037 | Long parameter list | Function has too many parameters | Group parameters into a struct |

### Complexity (W0011, W0018, W0039)

| Code | Title | When it appears | How to fix |
|------|-------|-----------------|------------|
| W0011 | High cyclomatic complexity | Function has too many branching paths | Split into simpler functions |
| W0018 | Deeply nested | Code has too many nesting levels | Extract inner blocks into functions |
| W0039 | Too many function arguments | Exceeds argument count limit | Group into struct or slice |

### Logic (W0015–W0017, W0019, W0028–W0032, W0036, W0038, W0040)

| Code | Title | When it appears | How to fix |
|------|-------|-----------------|------------|
| W0015 | Suspicious comparison | Comparison that is always true/false (e.g., `x == x`) | Review comparison logic |
| W0016 | Division by zero | Integer or float division by a literal zero | Add zero check before division |
| W0017 | Bitwise instead of logical | `&`/`|` used where `&&`/`||` likely intended | Use logical operators for boolean logic |
| W0019 | Out-of-bounds access | Array/list index may exceed bounds | Add bounds checking |
| W0028 | Unreachable code | Code after `return`, `break`, or `continue` | Remove unreachable code |
| W0029 | Empty branch | `if` or `match` branch with empty body | Fill or remove the branch |
| W0030 | Ineffective `break`/`continue` | Outside a loop | Remove or restructure |
| W0031 | Unused result of pure expression | Expression result discarded (e.g., `x + 1` alone) | Use the result or remove the expression |
| W0032 | Infinite loop | Loop without exit condition | Ensure the loop can terminate |
| W0036 | Redundant condition | `if cond { true } else { false }` | Use `cond` directly |
| W0038 | Panic in non-test code | `unreachable!` or `panic!` in production code | Handle the error case properly |
| W0040 | Suspicious discard | Return value of a function is discarded | Use `_ = fn()` if intentional |

### Memory (W0025)

| Code | Title | When it appears | How to fix |
|------|-------|-----------------|------------|
| W0025 | Potential memory leak | Dynamically allocated value never dropped | Ensure value is properly dropped or use owned types |

### Deprecated (W0010)

| Code | Title | When it appears | How to fix |
|------|-------|-----------------|------------|
| W0010 | Deprecated Syntax | Use of a syntax feature that has been replaced | Migrate to the modern equivalent (see SYNTAX.md or CHANGELOG) |

## Notes

- Codes E0002 and E0004 are reserved and not currently emitted.
- Warning codes W0001–W0040 are defined in the `DiagnosticCode` enum. Some codes may not be emitted yet pending implementation of the corresponding lint pass.
- Each warning belongs to a category (Unused, Type, Performance, Style, Complexity, Logic, Memory, Deprecated), allowing `deny_warnings` and `warning_filter` to filter by category.

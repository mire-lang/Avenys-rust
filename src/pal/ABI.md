# PAL ABI contract

PAL ABI version 4 is the boundary between Mire programs and host resource
operations. `pal.h` is the canonical declaration and semantic reference. Its
function comments define path policy, ownership, handle lifetime, and error
behavior. A symbol without an ownership annotation must not transfer memory.

## Compatibility rules

- Keep version 4 signatures, layouts, ownership rules, and failure behavior
  stable. A breaking change requires an explicit ABI major version increase.
- Additive symbols may be introduced within version 4 when they do not change
  existing behavior or layouts.
- Handles are generation-checked capabilities. Callers must not inspect or
  synthesize their fields.
- Sandbox decisions stay inside the PAL operation that reaches the host. A
  language-level pre-check is not a substitute for the filesystem gate.
- Returned PAL-owned memory must be released by its documented PAL function;
  borrowed pointers must never be freed by callers.

## Mire representation inventory

`pal_externs.mire` contains signatures whose C ABI types map directly to Mire's
current FFI types. `pal_externs.unsupported` records every declaration that
requires aggregate or callback ABI support. These files are generated from
`pal.h`; CI compares them with fresh Clang AST extraction. Do not hand-edit
either generated file.

Run `scripts/check-pal-abi.sh` after changing the header or extractor. The
compatibility harness lives in `palace/tests/pal_compat/`; a migrated symbol
must gain a differential fixture before its C implementation is removed.

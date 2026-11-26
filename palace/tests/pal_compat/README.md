# PAL compatibility harness

`compare.py` runs the same fixture command against the current C PAL and a
candidate Mire PAL, then compares exit status, standard output, and standard
error byte-for-byte. The fixture is responsible for selecting its backend;
the harness does not weaken sandbox settings or normalize error output.

Example:

```sh
python3 palace/tests/pal_compat/compare.py \
  --reference 'target/pal-compat-c/fs_case' \
  --candidate 'target/pal-compat-mire/fs_case'
```

Every migrated PAL symbol must have a fixture covering success, invalid input,
and ownership/error behavior where those cases apply. A fixture is added to the
matrix before its C implementation is replaced. The initial baseline remains
the C backend; the generated `pal_externs.unsupported` inventory makes every
ABI shape that still needs a Mire representation explicit.

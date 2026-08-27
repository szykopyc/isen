# Testing Isen programmes

`isen test` recursively runs files ending in `.test.is` below `tests/`. Each
file receives a fresh root scope and imports only what it names. Explicit `.is`
files run regardless of their filename, and explicit directories retain the
suffix rule.

```sh
./isen test
./isen test --profile logging
./isen test tests/stdlib.test.is
```

Named profiles and the default profile live in `isen.toml`. Their paths are
project-relative and may name files or directories. See
[PROJECT_CONFIG.md](PROJECT_CONFIG.md) for the schema. Supplying explicit paths
bypasses profile selection.

Every test is lexed, parsed, statically checked, and executed. A normal return
or `exit` passes; an unhandled problem or compilation error fails. All
selected files run, the command prints a summary, and it exits 1 if any failed.
Test programmes receive empty `Args` and `Kwargs`.

The optional `Test` extension supplies small assertions:

```is
borrow Test

Test.assert(2 + 2 == 4, "arithmetic")
Test.equal(String.lower("ISEN"), "isen", "lowercase conversion")
Test.not_equal(1, 2, "distinct values")
```

`Test.equal` and `Test.not_equal` require both values to have the same static
type and take a final string describing the assertion. `Test.fail(message)`
fails immediately. Assertions are ordinary extension calls rather than hidden
syntax, so production programmes do not import them accidentally.

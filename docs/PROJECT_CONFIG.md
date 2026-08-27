# Isen project configuration

Isen looks for the nearest `isen.toml`, starting beside the entry `.is` file and
walking towards the filesystem root. If none exists, the documented defaults
apply. Unknown sections and settings are errors so misspellings do not silently
change a build.

## Formatter

```toml
[format]
indent_width = 2       # 1 through 8
max_blank_lines = 1    # 0 through 4
final_newline = true
```

`isen --format` and `isen --format --check` load these settings independently
for each input file. Directory traversal ignores hidden directories, `target`,
and nested Git repositories.

## Test profiles

```toml
[test]
default_profile = "all"

[test.profiles.all]
paths = ["tests"]
fail_fast = false

[test.profiles.fast]
paths = ["tests/stdlib.test.is"]
fail_fast = true
```

Profile paths are resolved relative to `isen.toml`. Directory paths recursively
select `*.test.is`; explicitly named `.is` files are accepted regardless of
their suffix. Results are deduplicated and sorted before execution.

`isen test` uses `default_profile` when configured. `isen test --profile fast`
selects a named profile. Explicit paths such as `isen test tests/stdlib.test.is`
bypass profiles completely. Profile selection and explicit paths cannot be
combined, preventing an ambiguous implicit union. `fail_fast = true` stops
after the first failing test; it does not change what counts as a failure.

The configuration file is intended to remain the common, strict configuration
surface for Isen's own tools. Formatter and test-runner settings are supported
now; profiler or diagnostics settings should gain their own sections when they
have concrete, measurable options rather than accepting unused placeholders.

## Linked stashes

`stash_links` gives a stable, local name to a directory outside the project:

```toml
[stash_links]
linked = "../shared-stashes"
company = "/opt/company/isen"
```

The paths are relative to the directory containing `isen.toml`; absolute paths
remain absolute. The first path component is the alias:

```isen
borrow parse from "linked/parsing.is"
borrow policy from "company/security/policy.is"
```

Unaliased paths retain Isen's existing behaviour and resolve relative to the
stash that contains the `borrow`. Aliases always come from the entry project's
configuration, including inside borrowed stashes. A linked path cannot escape
its configured root with `..` or a symlink. Missing roots and files produce
normal file errors.

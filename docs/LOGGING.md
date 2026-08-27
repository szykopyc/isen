# Logging

Isen's logging library is implemented as ordinary Isen stashes. The native
runtime supplies only general-purpose capabilities: append-only file writes,
UTC time, canonical value display, and safe JSON construction.

There is no global logger. Configuration and structured context are explicit
arguments, file failures propagate as recoverable problems, and the minimum
level is one of `debug`, `info`, `warning`, or `error`.

## Human-readable logging

```isen
borrow logger from "stdlib/logging/human.is"
borrow info from "stdlib/logging/human.is"

dec log = logger("info", true, "logs/application.log")
info(log, "server started", #{ "host": "127.0.0.1", "port": "8080" })
```

The arguments to `logger` are minimum level, stdout enabled, and an optional
file path. Use `naught` instead of a path for stdout-only logging. The exported
level functions are `debug`, `info`, `warning`, and `error`; every call takes a
`map[string, string]` context, and `#{}` is the explicit empty context.

## Machine-readable logging

Import the same names from `stdlib/logging/json.is`. Each accepted event is one
compact JSON object followed by one newline:

```json
{"context":{"host":"127.0.0.1","port":"8080"},"level":"info","message":"server started","timestamp":"2026-08-26T14:32:10.123Z"}
```

Field and context ordering is deterministic. Messages and context values pass
through JSON constructors, so quoting and control characters are escaped
correctly.

## Optional colours

```isen
borrow logger from "stdlib/logging/colours.is"
borrow warning from "stdlib/logging/colours.is"
```

Only `colours.is` imports LengText. It colours the level label written to
stdout. When stdout and a file are both enabled, the file receives a separate
plain rendering and never receives ANSI escape sequences. The human and JSON
stashes do not import LengText, perform terminal detection, or enable colour
implicitly.

Paths in these examples are relative to a root-level programme. A programme in
another directory can use an appropriate relative path or an `isen.toml`
`stash_links` alias.

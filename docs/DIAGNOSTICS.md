# Isen diagnostics protocol

`isen --diagnostics <path>...` checks individual files or recursively checks
directories. Hidden directories and `target` are skipped, matching the
formatter. Output is always one JSON document on stdout when the inputs can be
collected.

```json
{
  "format": "isen-diagnostics-v1",
  "diagnostics": [
    {
      "path": "/absolute/path/programme.is",
      "line": 4,
      "column": 1,
      "end_line": 4,
      "end_column": 28,
      "severity": "error",
      "message": "unknown name 'value'"
    }
  ]
}
```

Paths are canonical absolute paths. Positions are one-based and end positions
are exclusive. The current lexer and parser retain line locations, so v1
diagnostics conservatively cover the complete source line; the column fields
allow future releases to become more precise without changing the transport.

The command exits 0 for a clean check, 1 when it reports diagnostics, and 2 for
invalid command usage. Input discovery failures are written to stderr and exit
1. Isen currently reports the first error found in each checked entry file;
errors in borrowed stashes carry the stash's actual path and are deduplicated.

Consumers must check `format` before interpreting the payload. Additive fields
within a known version should be ignored. A breaking semantic or structural
change will use a new format identifier.

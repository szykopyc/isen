# Profiling Isen programmes

Run a programme normally and print an Isen-aware profile to stderr:

```sh
./isen --profile examples/markov.is
```

The programme keeps its ordinary stdout and stderr behaviour. The profile is
printed after execution, including when execution fails. To retain a
machine-readable report for comparisons:

```sh
./isen --profile --json profile.json examples/markov.is
```

JSON reports use the versioned `isen-profile-v1` format. Durations are integer
nanoseconds, memory is bytes, counters are integers, and spans retain their
source path and line. This makes reports suitable for checking into benchmark
results or comparing before and after a runtime change.

## What is measured

The summary includes:

- wall time, user and system CPU time, approximate CPU utilisation, and peak
  resident memory on Unix platforms;
- source loading, lexing, parsing, static checking, runtime setup, and execution
  phases, including work performed for borrowed stashes;
- Isen function and native-call counts with inclusive and self time;
- the hottest source lines by execution count and time;
- statement and expression costs, including individual binary and unary
  operators;
- hot caller-to-callee edges;
- runtime counters for scope creation, variable reads and writes, lexical-scope
  lookup hops, loop iterations, collection construction and iteration snapshots,
  string concatenation and copied bytes, function closures, failures, raises, and
  recoveries.

`total` is inclusive: it contains time spent in child spans. `self` subtracts
instrumented child spans and points more directly at interpreter overhead in
that operation. Inclusive rows can overlap, so they should not be added
together to reconstruct wall time.

Peak RSS is the operating system's process high-water mark. The collection and
string counters describe Isen-level work rather than attempting to guess the
allocator's exact byte cost.

## Measurement discipline

Profiling instruments every statement and expression, so a profiled execution
is slower than an ordinary execution. Compare profiles produced by the same
Isen build mode and profiler version; use ordinary wall-clock benchmarks when
measuring the final speedup experienced by users.

Random, network, filesystem, terminal, and clock-driven programmes naturally
vary between runs. For performance comparisons, prefer a deterministic input,
run several samples, discard warm-up effects where relevant, and compare the
distribution rather than one duration.

The JSON report is deliberately raw rather than pre-aggregated. Tools can group
the `spans` array by category, name, source, or line without losing detail, and
can use `call_edges` to construct call graphs.

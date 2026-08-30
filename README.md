# Isen

Isen—Old English for iron—is a small, strongly typed application language. It
aims to keep the immediacy and readability of scripting without the ambiguity,
coercion, and runtime surprises of dynamic languages. Isen should resolve as
much as possible before execution, then run that knowledge fast—eventually
compiling a typed IR to native code—without forcing programmers to think like
systems programmers.

The guiding rule is: **simple semantics, explicit behaviour, strong guarantees,
fast execution, and no complexity unless it earns its place.** Its companion
rule is: **know what you import, and import only what you need.** Capabilities
such as JSON, UDP, TCP, HTTP, terminal control, and filesystem access are
separate opt-in spaces; importing one never silently imports another.

The interpreter is written in Rust, but Isen programs remain
Isen. Native code supplies practical boundaries such as sockets, files,
terminal input, clocks, typed arrays, and optional fused ML kernels. Control flow, model
architecture, training policy, and application logic stay in `.is` files.

## Build and run

Isen requires Rust 1.85 or newer. After cloning, build the release interpreter:

```sh
cargo build --release --locked
./isen --version
```

The launcher never downloads dependencies or builds implicitly. If the release
binary is absent, it prints the exact build command and exits. To install the
binary on your Cargo `PATH` instead, run `cargo install --path . --locked`.
Version 0.1.1 is distributed from this Git repository,
not crates.io.

Contributors can install the repository's versioned pre-commit and pre-push
checks with `sh scripts/install-git-hooks.sh`; see
[development checks](docs/DEVELOPMENT.md).

```sh
./isen examples/tour.is
```

Pushing a version tag such as `v0.1.1` publishes a checked GitHub Release with
Linux and macOS artifacts plus `SHA256SUMS`. The release workflow rejects a tag
that does not match the version in `Cargo.toml`. The Linux
`isen-linux-x86_64` artifact is a statically linked musl executable; the macOS
`isen-macos-arm64` artifact is a native Apple-Silicon executable.
Neither needs Rust or a separate Isen runtime installed. Both archives include
the executable, `stdlib/`, and `LICENSE`; keep `stdlib/` beside the executable
so bundled imports work from any directory. macOS still relies on the operating
system's standard system libraries, as every normal macOS command-line program
does.

Every file is statically checked before any statement executes. To check a
programme and all borrowed stashes without running it:

```sh
./isen --check examples/showcase.is
```

Programme arguments follow a literal `--`, keeping interpreter options and
Isen input separate:

```sh
./isen programme.is -- input.txt --mode=fast --verbose
```

Use `Args` for positional values and import `Kwargs` separately for keyword
values. `--name=value` stores the supplied string; bare `--flag` stores
`"true"`. Duplicate keyword names are rejected.

Run isolated test programmes from `tests/`:

```sh
./isen test
./isen test --profile logging
./isen test tests/stdlib.test.is
```

Directory discovery selects `*.test.is`; explicitly named `.is` files always
run. Named and default test profiles are configured in `isen.toml`. See
[the testing guide](docs/TESTING.md) for assertions, profiles, and exit behaviour.

To rebuild the release interpreter after changing it:

```sh
cargo build --release --locked
```

Format files or directories in place, or verify their formatting without
changing them:

```sh
./isen --format examples/showcase.is
./isen --format examples stdlib
./isen --format .
./isen --format --check .
```

The formatter uses canonical token spacing and configurable indentation. Put
settings and optional external stash aliases in `isen.toml`; see
[project configuration](docs/PROJECT_CONFIG.md). Directory formatting skips nested Git
repositories. It
preserves comments, blank lines, literal spelling, and intentional one-line
blocks. A file must parse before it is rewritten; formatting does not require
the programme to pass static type checking. Directory traversal is recursive;
hidden directories and `target` are skipped, and overlapping paths are
deduplicated.

Profile a programme with language-level timings and runtime counters:

```sh
./isen --profile examples/markov.is
./isen --profile --json profile.json examples/markov.is
```

The report covers compilation phases, Isen and native functions, hot source
lines, operations, call edges, scopes, lookups, loops, collections, string work,
failures, CPU time, and peak resident memory. See
[profiling](docs/PROFILING.md) for the measurement contract and JSON format.

Editors and CI can request versioned machine-readable diagnostics for files,
directories, or the current tree:

```sh
./isen --diagnostics programme.is
./isen --diagnostics .
```

See [diagnostics](docs/DIAGNOSTICS.md) for the generic protocol. The bundled
Neovim plugin publishes these as native editor diagnostics on open and save.

`./isen` is a small launcher for that release binary, so the normal command
stays `./isen program.is`.

For a precise, generation-oriented description of the language, see the
[language reference](docs/LANGUAGE_REFERENCE.md). It is intended for humans and
LLMs writing `.is` programs, with an interpreter-maintainer appendix. Run
`./isen --reference` after changing primitive types, operators, or extensions;
`./isen --reference --check` verifies generated sections in CI.
For optional Rust-backed spaces that do not require editing the interpreter,
see [extension authoring](docs/EXTENSIONS.md).

## Neovim (the joke is real)

The repository includes a small syntax-highlighting runtime plugin at
[`editors/nvim/isen.nvim`](editors/nvim/isen.nvim). Its
[LazyVim setup](editors/nvim/isen.nvim/README.md) is one local-plugin
entry; it detects `.is` files, highlights `@@` plus `$ ... \$` blocks, and
publishes compiler diagnostics on open and save.

Project-local generated state belongs under `.isen/`.

## The language

```is
dec names @@ list[string] = ["Ada", "Lin", "Mina"]
dec coordinates @@ arr[int] = @[10, 20, 30]
dec scores @@ map[string, int] = #{ "Ada": 10, "Lin": 12 }

form Player $
  name @@ string,
  score @@ int,
\$

given best(values @@ list[int]) @@ int $
  dec high @@ int = 0
  each value in values $
    if value > high $ high = value \$
  \$
  ret high
\$

space Maths $
  given twice(value @@ int) @@ int $ ret value * 2 \$
\$

dec player @@ Player = Player $ name: "Ada", score: Maths.twice(best([3, 8, 5])) \$
say(player.name, player.score)
```

### The style

Isen aims for readable intent without trying to resemble conversational
English. Its vocabulary is short, concrete, and slightly strange:

| Meaning | Isen form | How to read it |
| --- | --- | --- |
| Declaration | `dec answer @@ int = 42` | `answer` has type `int` |
| Function | `given add(...) @@ int` | given these arguments, produce an `int` |
| Data shape | `form Player $ ... \$` | form values into this named shape |
| Namespace | `space Maths $ ... \$` | keep related names in one space |
| Runtime package | `borrow LengText` | explicitly bring in a shipped space |
| While loop | `aslongas ready $ ... \$` | repeat as long as the condition holds |
| Loop exit | `enough` | leave the nearest loop |
| Loop skip | `onwards` | start the nearest loop's next iteration |
| Programme exit | `exit` | stop immediately and successfully |
| Conversion | `value.pour_into(string)` | explicitly change a scalar's type |
| Output | `say(value)` | show values as a line |
| Warning | `shout(value)` | report a problem and continue |
| Exception | `scream(value)` / `raise(value)` | raise a presented / quiet runtime failure |
| Recovery | `attempt $ ... \$ recover fault @@ Problem $ ... \$` | contain a runtime failure |
| Cleanup | `always $ ... \$` | run cleanup on every way out |
| Function result | `ret value` | finish with this value |
| No-result function | `given work() @@ unit $ ... \$` | fall through and produce `unit` |

`@@` is the type boundary: the name or function on its left has the type on its
right. `$ ... \$` is the block boundary. Braces are not overloaded for blocks,
maps, and data values; each shape remains visually distinct. Lists use `[...]`,
contiguous arrays use `@[...]`, maps use `#{...}`, and form values use
`Name $ ... \$`.

The marker is intentionally ASCII so it is equally easy to type on every
keyboard layout.

The odd words are mostly at declaration boundaries. Expressions intentionally
stay conventional: `a + b`, `thing.field`, `items[index]`, `work(value)`, and
`value.pour_into(string)` should not require a language-specific decoding ritual.
Types are exact, conversions are visible, and functions state both their input
and result types. The intended feel is a scripting language with fewer
surprises, not a systems language with shorter filenames.

`dec` is Isen’s declaration word. The annotation follows the name,
using `dec name @@ type = value`; omit `@@ type` when the value makes it obvious.
`say(...)` is ordinary output. `shout(...)` writes `shouting! : ...` to stderr
and lets the program continue. `scream(...)` writes `SCREAMING!!! : ...` as an
interpreter exception at that source line and stops the program unless an
enclosing `attempt` recovers it. All three accept any number of values and
render them with the same spacing rules. `print` is not a keyword.

Runtime recovery uses `attempt`, `recover`, and `always`:

```is
attempt $
  risky_work()
\$ recover problem @@ Problem $
  shout("recovered:", problem.message)
\$ always $
  close_things()
\$
```

`problem` is a local, statically typed `Problem` value. `recover` and
`always` are individually optional, but an `attempt` needs at least one of
them. `always` runs after ordinary completion, recovery, an unrecovered error,
`ret`, loop control, or `exit`. `exit` itself cannot be recovered.
Static type and name errors happen before execution, so runtime recovery cannot
hide them. See [examples/recovery.is](examples/recovery.is).

`Problem` is Isen's built-in failure form and always exposes `.message` as
`string`. Libraries can define typed descendants without introducing general
object inheritance:

```is
problem MachineJammed $
  gear @@ int
\$

scream(MachineJammed $ message: "teeth locked", gear: 4 \$)
```

The inherited `message` field is supplied when constructing the value and must
not be repeated in the declaration. A custom problem passed as the sole value
to `scream` retains its type. Multiple typed `recover` clauses are tried from
top to bottom; `Problem` is the fallback type and matches every custom problem.
Bounds, conversion, I/O, socket, and other ordinary runtime failures also
arrive as `Problem`. Custom problems are accepted wherever `Problem` is
expected, while retaining their concrete runtime type for recovery dispatch.

Bare `exit` stops the entire programme immediately with exit status 0. It
is intentionally not `exit()`: this is control flow, not a function call.
Use it when there is no more work to perform but nothing has failed. `scream`
remains the noisy, non-zero failure path. See
[examples/exit.is](examples/exit.is) for the smallest possible example.

A parenthesized `dec` can also bind and return a value inside an expression.
The binding lives in the surrounding scope and is visible in the loop body:

```is
aslongas (dec prompt @@ string = Input.line("you @@ ")) != "/quit" $
  say("heard", prompt)
\$
```

The initializer runs whenever the expression is evaluated. Consequently,
`aslongas (dec chatting @@ bool = true) == true` continually resets `chatting`
and is intentionally an infinite loop.

### Core choices

- `int`, `float`, `bool`, `string`, and `naught` are scalar types. `naught` is
  both the absence-value literal and its exact type. A value that may be absent
  says so explicitly: `dec nickname @@ perchance[string] = naught`. It can later
  hold `string`, return to `naught`, and be compared with either. Comparing a
  named optional with `naught` narrows it inside the appropriate `if`, `else`,
  or `aslongas` branch. Explicit `.pour_into(string)` remains available when
  control flow has not established that the value is present.
- `list[type]` and fixed-length `arr[type]` are fully typed and can nest freely.
  Arrays use `@[...]`; they are backed by contiguous Rust storage. Expected
  types make empty and nested-empty literals practical, for example
  `dec rows @@ list[list[int]] = [[], [1, 2]]` and
  `dec weights @@ arr[float] = @[]`.
- Maps use `#{ ... }` so map literals cannot be confused with form values.
  Their keys may be `int`, `bool`, or `string`; ordering is not part of the
  language contract. A typed empty map is simply
  `dec counts @@ map[string, int] = #{}`.
- Variables, function parameters, function returns, fields, and container
  elements are checked for exact types before execution. Both sides of every
  branch are checked, even when one side is unreachable. Runtime validation
  remains for bounds, conversions, I/O, sockets, and native boundaries.
- Integer arithmetic is checked and raises `Problem` on overflow in every
  build. Expressions and arguments evaluate left-to-right; `&&` and `||`
  short-circuit.
- Equality is structural for ordinary values and aggregates. Runtime resources
  such as sockets are not comparable, although an optional resource may be
  compared with `naught`.
- Lists, arrays, maps, forms, and problems are shared mutable aggregates across
  assignment, calls, and returns. `List.push` creates a shallow outer copy;
  nested aggregates remain shared.
- Use `.pour_into(type)` for deliberate scalar conversion, for example
  `dec id @@ int = "42".pour_into(int64)` or
  `dec label @@ string = id.pour_into(string)`. There is no mirrored `pour_from`:
  it would perform the same conversion with the operands visually reversed.
- `$ ... \$` delimits a block; `each item in list`, `aslongas`, `if`, `else`,
  `attempt`, `recover`, `always`, `enough`, `onwards`, `exit`, and `ret`
  are built in. `enough` leaves the
  nearest loop, `onwards` skips to its next iteration, and `exit` leaves the
  entire programme.
- `say` is normal output, `shout` is a non-fatal warning, and `scream` raises a
  recoverable runtime failure. Warnings and exceptions use stderr.
- `given` has typed arguments and a required result type. A function returning
  `unit` may fall through without `ret`; non-unit functions may not. `space` groups
  functions with dot access, such as `Maths.twice(4)`.
- Shipped runtime spaces are opt-in: use `borrow Maths`, `borrow Tcp`, `borrow
  Json`, and so on before referring to them. The available shipped spaces are
  `Args`, `Array`, `Bytes`, `Env`, `File`, `Http`, `Input`, `Json`, `Keyboard`,
  `Kwargs`, `LengText`, `List`, `Map`, `Maths`, `Ordering`, `Path`, `Queue`,
  `Random`, `Range`, `Stack`, `String`, `Tcp`, `Test`, `Time`, and `Udp`. `say`, `shout`, `scream`,
  `raise`, and `size`
  remain language-level facilities.
- Isen libraries are called stashes. A stash keeps declarations private
  unless it names them with `share`, and callers use
  `borrow function_name from "path.is"`. Paths are relative to the borrowing
  file; stashes are evaluated once and cached by canonical path.
- Form names begin with an uppercase letter. Values are created with
  `Player $ field: value \$` and accessed with `player.field`.
- `form` declares a named data shape; `struct` is not a keyword.
- The shipped `Time` space provides `Time.clock()` (monotonic milliseconds),
  `Time.since(start)`, and `Time.sleep(milliseconds)`.
- `Random.int(low, high)` produces a bounded pseudo-random integer;
  `Random.seed(value)` makes subsequent random calls reproducible.
- `Args`, `Kwargs`, `Env`, and `Path` expose typed, explicit process and filesystem
  inspection. Directory listings are sorted for deterministic results.
- `Json` keeps heterogeneous JSON in an opaque `json` value and exposes typed
  extraction, safe construction, and compact or indented encodings.
- The logging stashes provide explicit human, JSON-lines, and coloured logging
  with stdout and file sinks. See [logging](docs/LOGGING.md).
- `LengText.flush()` clears and flushes an ANSI-capable terminal after
  `borrow LengText`; it is namespaced so `flush` remains available for buffer
  libraries and application code.
- `LengText.blit(frame)` repaints an already-composed frame in one stdout write;
  `screen_begin` and `screen_end` bracket cursor-safe animation.
- `Input.line(prompt)` provides ordinary blocking line input. The separate
  `Keyboard` space provides blocking, timed, and immediate raw keypresses for
  console games; see [examples/keyboard.is](examples/keyboard.is).
- `Bytes`, `Udp`, `Tcp`, and `Http` are separate networking capabilities.
  Binary payloads use checked `arr[int]` bytes; JSON remains separate and must
  be borrowed explicitly. See [Networking](#networking).

Statements are separated naturally by their shape; semicolons are optional.
Use `//` or `#` for comments. See [examples/tour.is](examples/tour.is) for a
runnable tour, or [examples/showcase.is](examples/showcase.is) for a complete
worked program combining the major language features.

## Stashes

A stash is just another `.is` file. Declare normally, then explicitly share the
small public surface:

```is
// lib/temperature.is
given celsius_to_fahrenheit(value @@ float) @@ float $
  ret value * 1.8 + 32.0
\$
share celsius_to_fahrenheit
```

Borrow a shared name from a path relative to the current file:

```is
borrow celsius_to_fahrenheit from "lib/temperature.is"
say(celsius_to_fahrenheit(20.0))
```

Conflicting shared names can be given local aliases:

```is
borrow parse from "json.is" as parse_json
borrow parse from "config.is" as parse_config
```

Aliases currently apply to shared values, functions, and namespaces. Nominal
forms and problems retain their shared names.

Stashes are isolated, run once, and may share values, functions, spaces, and
forms. There is no wildcard borrowing. See
[examples/library_app.is](examples/library_app.is) and
[examples/lib/temperature.is](examples/lib/temperature.is) for a runnable
multi-file example.

## Networking

Networking is deliberately split by capability:

- `Bytes` converts UTF-8 strings to checked `arr[int]` bytes and back.
- `Udp` supports hostnames, IPv4 and IPv6, connected or addressed sends,
  binary datagrams, readiness, broadcast, and nonblocking mode.
- `Tcp` exposes listeners and streams, connect timeouts, blocking and
  nonblocking accept, binary/text reads, partial or complete writes, shutdown,
  readiness, Nagle control, and local/peer addresses.
- `Http` performs bounded plain HTTP/1.1 requests and returns an
  `http_response` with status, reason, version, headers, binary body, and an
  optional UTF-8 `.text` view.

`Http` does not import or parse JSON. A programme that wants JSON says so:

```is
borrow Http
borrow Json

dec response @@ http_response = Http.get(
  "example.test", 80, "/data", #{ "Accept": "application/json" },
  5000, 1048576)
if response.text == naught $ raise("response is not UTF-8") \$
dec document @@ json = Json.parse(response.text.pour_into(string))
```

The native layer supplies transport primitives, not application policy.
Routing, retries, authentication, framing, connection pools, protocol clients,
and server libraries can be ordinary Isen stashes built on `Tcp`, `Udp`, and
`Bytes`. `Http` currently means unencrypted HTTP/1.1; HTTPS/TLS is not silently
approximated. See [examples/server.is](examples/server.is),
[examples/tcp_server.is](examples/tcp_server.is), and
[examples/http.is](examples/http.is).

## Optional colour

`LengText` is deliberately absent from the default runtime. Borrow it in the
scope that wants ANSI terminal colour:

```is
borrow LengText

say(LengText.green("ready"))
say(LengText.blue("information"))
shout("careful")
scream("absolutely not")
```

The small palette is `LengText.red(value)`, `.yellow(value)`, `.green(value)`,
and `.blue(value)`. Each returns coloured `string`, so it composes with `say`,
`shout`, and ordinary strings. Once borrowed, the `shouting!` label is yellow
and the `SCREAMING!!!` label is red; only the label is coloured. A borrowing is
lexical and affects its scope and child scopes. Without `borrow LengText`, the
space is unknown and shout/scream retain their plain stderr output.
`LengText.flush()` clears the terminal and flushes stdout for animated output.

## UDP example

Run `./isen examples/server.is` to start the small binary-safe UDP echo example.

## Spinning torus

Run `./isen examples/donut/donut.is` for a lit, depth-buffered ASCII torus.
The projection, lighting, framebuffer, animation, and frame pacing are Isen.
It is also a worked stash example: the programme borrows a canvas and a
precomputed trigonometry wheel from its local `lib/` directory.

## Tiny Markov model

Run `./isen examples/markov.is` for a small weighted word-level Markov chain.
The transition branches are the model; `Random.int(0, 99)` samples each next
word according to its encoded transition weight.

For a learned version, run `./isen examples/learned_markov.is`. The Markov
algorithm itself is Isen: it tokenizes a supplied corpus, scans it to
count observed followers, samples one with `Random.int`, and assembles two
paragraphs. It uses two-word transitions most of the time and intentionally
backs off to one-word transitions occasionally for stranger recombinations.
The model uses the general `String` operations for lossless word/symbol tokens,
character indexing, slicing, splitting, finding, and joining.

## Tiny neural language model

Run `./isen examples/tiny_lm.is` for a trainable softmax bigram language model.
The model, gradient calculation, SGD loop, and sampling are all Isen.
The interpreter contributes only general numerical primitives: `float`,
`Array.float(length, initial)`, mutable array slots, `Random.float`, and
`Maths.exp`/`log`/`tanh`/`sqrt`/`sin`/`cos`, plus `abs`, `floor`, `min`, `max`,
and `pow`. Common constants are values such as `Maths.pi`, `Maths.tau`, and
`Maths.e`.

For an external corpus, run `./isen examples/word_lm.is -- corpus.txt`. It is a compact
neural four-gram model: three ordered preceding words enter separate embedding
tables, feed a `tanh` hidden layer, and score the next word. The vocabulary
builder, negative-sampling trainer, backpropagation, SGD loop, and sampler are
Isen. `String.lower`, `File.read`, and `List.push` are general-purpose
helpers. It keeps the 256 most frequent words plus `unk`, which makes modest
corpora useful without making an interpreted training run impractical.

The fused neural kernels are not part of the default interpreter. Build the
optional extension first with `cargo build --release --features ml-kernels`.
It appears as the explicit `ML` space rather than enlarging `Array`.

For a larger model, run `./isen examples/mlp_lm.is -- corpus.txt`. It keeps 1,024 words and
uses three position-specific projection matrices, negative sampling, and an
observed four-gram continuation table. The model architecture, training loop,
fallback policy, and generation are Isen. Rust supplies fused typed-array
kernels for dense projection, sampled updates, backpropagation, and stable
softmax sampling. Its full-book preset uses a width of 24, nine negative
samples, six epochs, and a decaying learning rate.

Run `./isen examples/gru_lm.is -- corpus-directory` for the recurrent version. Its update gate,
reset gate, paragraph resets, sampled-softmax schedule, learning-rate schedule,
prompt priming, and generation loop are expressed in Isen. The fused
native kernels perform typed-array GRU arithmetic while the script caches
activations and schedules truncated backpropagation through time.

The GRU reads every `.txt` file directly inside the supplied directory, in
sorted filename order. Training data is deliberately not part of this
repository. Its larger preset uses a 4,096-word vocabulary, width 64,
a 24-token backpropagation window, and normalized sampled-softmax updates over
one observed continuation plus nine unique alternatives. During training it
checkpoints the vocabulary and learned arrays in `.isen/gru_checkpoint` after every
completed epoch. After epoch ten it opens an interactive prompt. Later, run
`./isen examples/gru_chat.is` to reload the last completed checkpoint and
prompt it without reading the corpus or training again. Type `/quit` to leave
either prompt.

## License

Isen is licensed under the [Apache License 2.0](LICENSE).

# Isen Language Reference

Status: implementation reference for Isen 0.1.x.

Audience: programmers, code-generating LLMs, and interpreter maintainers. This
document describes the language that exists in `src/main.rs`, not a proposed
ideal language. When this document and the interpreter disagree, the
interpreter is authoritative and this document should be corrected.

## Quick generation contract

To generate valid Isen, remember these rules first:

1. Files use the `.is` extension and run with `./isen path.is`.
   `./isen --check path.is` checks the complete stash graph without running it.
   `./isen --format <path>...` formats files and directories in place, while
   `./isen --format --check <path>...` verifies canonical formatting for CI.
   Use `.` to format every `.is` file below the current directory.
   `./isen --profile path.is` runs with an Isen-aware performance report;
   add `--json report.json` before the programme path to retain raw data.
   `./isen --diagnostics <path>...` emits versioned JSON for editors and CI.
   Pass programme arguments only after `--`, as in `./isen app.is -- one two`.
   `./isen test` runs `*.test.is` below `tests/`; explicit files and directories
   may follow the command.
2. Blocks begin with `$` and end with `\$`. Do not use braces or `end`.
3. `@@` separates a name from its type.
4. Declare variables with `dec`, functions with `given`, named data with
   `form`, and namespaces with `space`.
5. Every function parameter and return is explicitly typed.
6. Shipped namespaces do not exist until explicitly borrowed, for example
   `borrow Maths`.
7. Use `say`, `shout`, and `scream`, not `print`. Use `raise` when a recovered
   failure must not acquire `scream`'s presentation label.
8. Use `aslongas`, not `while`; `each`, not `for`; `ret`, not `return`;
   `enough`, not `break`; and `onwards`, not `continue`.
9. Use `.pour_into(type)` for explicit scalar conversion.
10. Use `form`, never `mold` or `struct`.
11. Recover runtime failures with `attempt`, `recover`, and `always`.

A small complete program:

```is
borrow Maths
borrow LengText

form Person $
  name @@ string,
  score @@ float,
\$

given rating(person @@ Person) @@ string $
  if person.score >= 8.0 $ ret "excellent" \$
  ret "ordinary"
\$

dec person @@ Person = Person $ name: "Ada", score: Maths.sqrt(81.0) \$
say(person.name, LengText.green(rating(person)))
```

## Lexical rules

- Identifiers match `[A-Za-z_][A-Za-z0-9_]*` and are case-sensitive.
- Integer literals are unsigned decimal digits in source. Negative values use
  unary `-`, for example `-12`.
- Float literals require digits on both sides of the decimal point, such as
  `1.0`. Scientific notation is not supported.
- String literals use double quotes. Supported escapes are `\n`, `\t`, `\"`, and
  `\\`. Multiline string literals are not supported.
- `// comment` and `# comment` run to the end of the line.
- `#{` starts a map literal, so it is not a comment.
- Semicolons are optional. Newlines are whitespace, not tokens; statement
  shapes normally determine where statements end.
- Source locations currently track line numbers, not columns.

## Grammar

The following EBNF is descriptive. `IDENT`, `INT`, `FLOAT`, and `TEXT` are the
tokens described above.

```ebnf
program       = { statement [ ";" ] } ;
block         = "$" { statement [ ";" ] } "\$" ;

statement     = borrowing
              | sharing
              | declaration
              | output
              | conditional
              | while_loop
              | each_loop
              | function
              | return
              | attempt
              | "enough"
              | "onwards"
              | "exit"
              | namespace
              | form
              | problem
              | assignment
              | expression ;

borrowing     = "borrow" IDENT [ "from" TEXT [ "as" IDENT ] ] ;
sharing       = "share" IDENT ;
declaration   = "dec" IDENT [ "@@" type ] "=" expression ;
output        = ( "say" | "shout" | "scream" | "raise" )
                "(" [ expression { "," expression } ] ")" ;
conditional   = "if" expression block
                [ "else" ( conditional | block ) ] ;
while_loop    = "aslongas" expression block ;
each_loop     = "each" IDENT "in" expression block ;
function      = "given" IDENT [ generic_parameters ] "(" [ parameters ] ")"
                "@@" type block ;
generic_parameters
              = "[" IDENT { "," IDENT } [ "," ] "]" ;
parameters    = parameter { "," parameter } ;
parameter     = IDENT "@@" type ;
return        = "ret" expression ;
attempt       = "attempt" block
                { "recover" IDENT "@@" type block }
                [ "always" block ] ;
namespace     = "space" IDENT block ;
form         = "form" IDENT "$" [ form_fields ] "\$" ;
form_fields  = form_field { [ "," | ";" ] form_field } [ "," | ";" ] ;
form_field   = IDENT "@@" type ;
problem      = "problem" IDENT "$" [ form_fields ] "\$" ;
assignment    = assignable assignment_operator expression ;
assignment_operator
              = "=" | "+=" | "-=" | "*=" | "/=" | "%="
              | "&=" | "|=" | "^=" | "<<=" | ">>=" ;
assignable    = IDENT | postfix "[" expression "]" | postfix "." IDENT ;

(* BEGIN GENERATED:SOURCE_TYPES *)
type          = "int" | "int64" | "float" | "bool" | "string" | "json" | "naught" | "unit" | "udp_socket" | "udp_packet" | "tcp_listener" | "tcp_stream" | "http_response" | IDENT
              | "perchance" "[" type "]"
              | "list" "[" type "]"
              | "arr" "[" type "]"
              | "map" "[" type "," type "]" ;
(* END GENERATED:SOURCE_TYPES *)

expression    = binary_expression ;
declaration_value
              = "dec" IDENT [ "@@" type ] "=" expression ;

primary       = INT | FLOAT | TEXT | "true" | "false" | "naught" | "unit" | IDENT
              | declaration_value
              | list_literal | array_literal | map_literal | form_value
              | "(" expression ")" ;
list_literal  = "[" [ expression { "," expression } [ "," ] ] "]" ;
array_literal = "@[" [ expression { "," expression } [ "," ] ] "]" ;
map_literal   = "#{" [ expression ":" expression
                { "," expression ":" expression } [ "," ] ] "}" ;
form_value   = IDENT "$" [ IDENT ":" expression
                { "," IDENT ":" expression } [ "," ] ] "\$" ;

postfix       = primary { call | index | field | conversion } ;
call          = "(" [ expression { "," expression } ] ")" ;
index         = "[" expression "]" ;
field         = "." IDENT ;
conversion    = ".pour_into(" type ")" ;
```

Form value parsing requires the form name to begin with an uppercase letter.
Use `Person $ ... \$`, not `person $ ... \$`.

## Operators

From lowest to highest precedence:

<!-- BEGIN GENERATED:OPERATORS -->
| Precedence | Operators | Valid operands |
| --- | --- | --- |
| 1 | `\|\|` | bool, bool |
| 2 | `&&` | bool, bool |
| 3 | `==`, `!=` | compatible equality-supporting values |
| 4 | `<`, `<=`, `>`, `>=` | matching int, float, or string |
| 5 | `<<`, `>>` | int, int; shift count 0 through 63 |
| 6 | `+`, `-`, `\|`, `^` | matching arithmetic values; bitwise operators require int |
| 7 | `*`, `/`, `%`, `&` | matching arithmetic values; bitwise operators require int |
| unary | `-`, `!`, `~` | numeric negation; boolean negation; integer complement |
<!-- END GENERATED:OPERATORS -->

Binary operators are left-associative. There are no implicit numeric
promotions: `1 + 2.0` is an error. Integer division truncates toward zero.
Division and remainder by zero are errors. Integer addition, subtraction,
multiplication, division, remainder, unary negation, and left shift are checked;
a result outside signed 64-bit range raises `Problem` rather than wrapping.
Shift counts outside 0 through 63 are errors. Float arithmetic retains IEEE-754
semantics.

`==` and `!=` use structural equality for `int`, `float`, `bool`, `string`,
`naught`, `unit`, JSON, lists, arrays, maps, forms, and problems, including
nested and cyclic aggregates. Both operands must have compatible types.
Network resources and aggregates containing them cannot be compared. A
`perchance` resource may still be compared with `naught` to test presence.

Expression evaluation is strictly left-to-right. This includes function
arguments, binary operands, collection entries, form fields in their written
order, indexes, and assignment targets. `false && right` and `true || right`
do not evaluate `right`.

Compound assignment evaluates its target once and captures its current value
before evaluating the right-hand expression. Indexed list/array/map slots,
nested slots such as `grid[y][x]`, and fields such as `scene.cells` are mutable.

## Generic functions and ordering

Generic parameters must be declared explicitly after the function name. Their
names use uppercase letters, digits, and underscores. An undeclared type name
is always an error, even when it is all-uppercase. Types are inferred at each
call and must resolve consistently:

```is
given first[T](values @@ list[T]) @@ T $ ret values[0] \$
```

Generic types may be nested in `list`, `arr`, `map`, and `perchance`. Ordering
is deliberately explicit in generic code: import `Ordering` and use
`Ordering.less(a, b)` or `Ordering.compare(a, b)`. The shipped ordering covers
`int`, `float` (except NaN), and lexicographic `string` values.

## Types and values

### Scalars

- `int` is a signed 64-bit integer. `int64` is a source-level alias for `int`.
- `float` is an IEEE-754 64-bit float.
- `bool` contains `true` or `false`.
- `string` is owned UTF-8 string.
- `naught` is the absence value and its exact standalone type.
- `unit` is the singleton result of work that produces no meaningful value.

### Optional values

`perchance[T]` accepts either `T` or `naught`:

```is
dec nickname @@ perchance[string] = naught
nickname = "Sonya"
if nickname != naught $
  dec definite @@ string = nickname
  say(nickname)
\$
```

Pouring a present `perchance[T]` into `T` unwraps it. Pouring `naught` into
`T` is a runtime error. `perchance[naught]` and nested `perchance` types are
rejected. A simple comparison with `naught` narrows a named optional inside the
corresponding branch: `!=` narrows the true branch to `T` and the false branch
to `naught`; `==` does the reverse. Either operand order is accepted, and the
true branch of an `aslongas` condition is narrowed as well. Assignment is still
checked against the binding's declared `perchance[T]` type and updates the
current refinement.

### Lists

`list[T]` is a typed growable sequence, written `[a, b, c]`. Lists may nest.
Indexing is zero-based and bounds-checked, and list slots are mutable.
`List.push(list, item)` remains the functional copy-producing operation;
`List.append`, `List.pop`, and `List.shift` mutate the shared list.

Empty list literals take their type from context:

```is
dec seen @@ list[int] = []
dec groups @@ list[list[string]] = [[], ["known"]]
```

An unannotated standalone `[]` is rejected because it has no element type to
infer. Mixed `T` and `naught` elements infer `list[perchance[T]]`.

### Arrays

`arr[T]` is contiguous, fixed-length storage, written `@[a, b, c]`. Array slots
are mutable and bounds-checked:

```is
dec weights @@ arr[float] = @[0.0, 1.0]
weights[0] = 0.5
```

Typed empty arrays use the same contextual rule: `dec values @@ arr[float] =
@[]`. `Array.sized(length, initial)` constructs `arr[T]` for any inferred `T`;
`Array.float` and `Array.int` remain convenient numeric constructors.

### Maps

`map[K, V]` is a typed deterministically ordered mapping, written `#{ key: value }`. Keys may
be `int`, `bool`, or `string`; `float` and compound keys are rejected. Map slots
are mutable. Direct indexing of a missing key is an error; `Map.get` returns
`perchance[V]` instead. `each key in values` and `Map.keys(values)` traverse
keys in ascending key order.

Typed empty maps are contextual: `dec counts @@ map[string, int] = #{}`.
`Map.string_int()` remains available when inference comes from a call rather than
an annotation.

### Forms

A form declares an exact named product type:

```is
form User $
  name @@ string,
  age @@ int,
  note @@ perchance[string],
\$

dec user @@ User = User $ name: "Ada", age: 36, note: naught \$
say(user.name)
```

Construction requires every declared field exactly once. Field types are
checked. Fields can be read and reassigned with dot access, including through
nested targets such as `scene.cells[i] = 0`.

### Problems

`Problem` is the built-in base failure type and has one field, `message @@
string`. A custom problem is a specialised form that implicitly inherits that
field:

```is
problem InvalidReading $
  sensor @@ string,
  value @@ float
\$

dec failure @@ InvalidReading = InvalidReading $
  message: "temperature is outside the sensor range",
  sensor: "roof",
  value: 900.0
\$
```

Do not redeclare `message`. `scream(failure)` preserves the custom type when a
problem is its sole argument. Ordinary `scream(...)` calls and runtime failures
produce base `Problem` values. A custom problem may be assigned or passed where
`Problem` is expected; its concrete type remains available to recovery
dispatch. `Problem` itself is a reserved type name.

### Unit and runtime resource types

`unit` is a proper source type and predefined singleton value. A local binding
or parameter may shadow the value name—for example, a string parameter naming
a measurement unit—without affecting the type spelling. A `@@ unit` function may
fall through its closing `\$`, which implicitly returns `unit`, or use `ret
unit` explicitly:

```is
given announce(name @@ string) @@ unit $
  say("hello", name)
\$
```

`udp_socket`, `udp_packet`, `tcp_listener`, `tcp_stream`, and `http_response`
are opaque networking types produced only by their explicit spaces. A UDP
packet exposes read-only `.host` (`string`), `.port` (`int`), `.bytes`
(`arr[int]`), and `.text` (`perchance[string]`) fields. An HTTP response exposes
`.status`, `.reason`, `.version`, `.headers`, `.body`, and `.text`.

## Type checking and conversion

A static pass checks the complete programme and every borrowed stash before
execution begins. Both sides of conditionals and loop bodies are checked even
when a path will not execute. Runtime checks remain for values that genuinely
depend on execution, such as indexes, conversions, I/O, sockets, and native
boundaries.

- Annotated declarations, assignments, function arguments, returns, form
  fields, and collection elements are checked.
- There are no implicit `int`/`float` or scalar/string conversions.
- `perchance[T]` is the one explicit compatibility rule: it accepts `T` and
  `naught` without changing the underlying data.
- Collection literals infer one common element type. Mixing `naught` with one
  other type produces `perchance[that_type]`.

Supported `.pour_into(...)` conversions:

| Source | Destination |
| --- | --- |
| `string` | `int`, `float`, `bool` |
| `int` | `float`, `string` |
| `float` | `int`, `string` |
| `bool` | `string` |
| `perchance[T]` containing a value | `T` |
| any `T` or `naught` | compatible `perchance[T]` |

Float-to-int uses Rust's `as i64` semantics. String-to-bool accepts exactly
`"true"` and `"false"`.

## Scope and execution

- The file starts in a root lexical scope.
- Every `$ ... \$` control-flow block executes in a child scope.
- A declaration belongs to its current scope. Assignment searches outward and
  updates the nearest existing binding.
- Functions are closures over the scope where `given` executes.
- Function calls create a child scope containing typed parameters.
- A `space` body executes immediately in its own scope. Its resulting names are
  accessed with dot notation.
- An `each` loop evaluates its collection once and iterates over a snapshot.
  Lists and arrays yield elements; maps yield keys in deterministic order.
  Mutating the collection during the loop does not change that loop's items,
  so appended list values and newly inserted map keys are not visited.
- A parenthesised declaration expression binds in the surrounding evaluation
  scope and returns the new value. Its initializer runs every time the
  expression is evaluated.

### Value and aliasing law

`int`, `float`, `bool`, `string`, `naught`, `unit`, and JSON behave as values.
JSON has no mutable source operations. Lists, arrays, maps, forms, and problems
are shared mutable aggregates. Assigning one to another name, passing it to a
function, or returning it preserves the same aggregate identity; mutation
through either name is visible through the other.

`List.push(values, item)` is the explicit exception: it creates a new outer
list and leaves `values` unchanged. The copy is shallow, so aggregate elements
inside both lists remain shared. No operation performs an implicit deep copy.

## Control flow

```is
if condition $
  // ...
\$ else if other $
  // ...
\$ else $
  // ...
\$

aslongas condition $
  // ...
\$

each item in items $
  // ...
\$
```

- Conditions must evaluate to `bool`.
- `ret value` returns from the current function.
- A `@@ unit` function implicitly returns `unit` when execution reaches its
  closing `\$`; every other return type still requires an exiting path.
- `enough` leaves the nearest `each` or `aslongas` loop.
- `onwards` skips to the nearest loop's next iteration.
- `enough` and `onwards` outside a loop are errors. They do not cross a
  function boundary.
- `exit` stops the entire programme immediately with exit status 0.
- `scream(...)` raises a runtime failure with a source-line diagnostic.
- `raise(...)` raises the same typed failure without adding scream presentation.

Runtime failures can be contained without borrowing a package:

```is
attempt $
  dangerous_work()
\$ recover problem @@ Problem $
  shout(problem.message)
\$ always $
  clean_up()
\$
```

- `recover` binds a statically typed problem value.
- Recovery clauses are tested in source order. A custom type matches itself;
  `Problem` matches every custom problem and should normally come last.
- A duplicate clause, or any clause after `Problem`, is statically unreachable
  and rejected.
- `recover` or `always` may be omitted, but not both.
- `always` runs on normal completion, recovered and unrecovered failures,
  `ret`, `enough`, `onwards`, and `exit`.
- A failure or control-flow instruction inside `always` supersedes the pending
  result, matching cleanup-block semantics in familiar languages.
- `exit` bypasses `recover`, though `always` still runs.
- Failures raised inside `recover` are not recovered again by the same block.
- Static checker failures occur before execution and cannot be recovered.

## Output and failure

- `say(values...)` writes values separated by spaces to stdout, followed by a
  newline.
- `shout(values...)` writes `shouting! : ...` to stderr and continues.
- `scream(values...)` produces `SCREAMING!!! : ...` and raises a runtime
  failure; it stops the programme when no enclosing `attempt` recovers it.
- `raise(values...)` raises quietly, which is preferable for expected failures
  that an `attempt` intends to recover.
- After `borrow LengText`, shout and scream colour only their labels yellow and
  red respectively.
- After `borrow LengText`, `LengText.flush()` emits ANSI clear-screen and
  cursor-home codes, then flushes stdout.
- `LengText.blit(frame)` moves home and replaces the visible frame in one
  locked write, without exposing a cleared intermediate screen.
- `size(value)` returns character count for string or element count for a list,
  array, or map.

## Borrowing, sharing, and shipped packages

Every shipped namespace is absent until borrowed in the current lexical scope:

```is
borrow Maths
dec value @@ float = Maths.sqrt(9.0)
```

The borrowing is visible in that scope and its children. Available shipped spaces:

`Args`, `Array`, `Bytes`, `Env`, `File`, `Http`, `Input`, `Json`, `Keyboard`,
`Kwargs`, `LengText`, `List`, `Map`, `Maths`, `Ordering`, `Path`, `Queue`,
`Random`, `Range`, `Stack`, `String`, `Tcp`, `Test`, `Time`, and `Udp`.

An Isen library file is called a stash. Declarations are private unless
the stash explicitly shares them after declaration:

```is
given parse_record(line @@ string) @@ string $ ret line $
share parse_record
```

Another file borrows a shared name with a path relative to itself:

```is
borrow parse_record from "lib/records.is"
```

A shared value, function, or namespace may be bound under a local alias:

```is
borrow parse from "json.is" as parse_json
borrow parse from "config.is" as parse_config
```

Aliases are lexical and must not duplicate another name in the same scope.
Shipped extension imports are not aliased. Nominal form and problem aliases are
also deliberately excluded in v0.1.0 because renaming a nominal type would
otherwise obscure its identity; borrow those by their shared declaration name.

Each canonical stash path is evaluated once per programme. Private names remain
available to shared function closures but cannot be borrowed directly. Circular
borrowing is rejected with the path chain.

## Shipped package API

Signatures below use `->` only as documentation notation; `->` is not
Isen syntax.

### `Time`

```text
Time.clock()                         -> int  // monotonic milliseconds
Time.since(start @@ int)             -> int
Time.sleep(milliseconds @@ int)      -> unit
Time.unix_millis()                   -> int  // Unix epoch milliseconds
Time.utc()                           -> string  // RFC 3339 UTC
```

Sleep durations must be non-negative.

### `Random`

```text
Random.int(low @@ int, high @@ int)          -> int
Random.float(low @@ float, high @@ float)    -> float
Random.seed(value @@ int)                    -> unit
```

Bounds are inclusive for integers. Low must not exceed high. This is a small
pseudo-random generator, not cryptographic randomness. Calling `seed` resets
the generator, so the same seed followed by the same calls produces the same
values across runs of this Isen version.

### `Args`, `Kwargs`, `Env`, and `Path`

```text
Args.all()                         -> list[string]
Args.get(index @@ int)             -> perchance[string]
Kwargs.all()                       -> map[string, string]
Kwargs.get(name @@ string)         -> perchance[string]
Kwargs.has(name @@ string)         -> bool
Env.get(name @@ string)            -> perchance[string]
Env.read(path @@ string)           -> perchance[map[string, string]]

Path.current()                     -> string
Path.join(base @@ string, child @@ string) -> string
Path.exists(path @@ string)          -> bool
Path.is_file(path @@ string)         -> bool
Path.is_dir(path @@ string)          -> bool
Path.canonical(path @@ string)       -> string
Path.list(directory @@ string)       -> list[string]
Path.name(path @@ string)            -> perchance[string]
Path.parent(path @@ string)          -> perchance[string]
```

Arguments are the values after the command-line `--`; the programme path is
not included. Tokens shaped as `--name=value` go to `Kwargs`; bare `--flag`
means `"true"`; everything else goes to `Args`. Keyword names use ASCII letters,
digits, `_`, or `-`, and duplicates are a command-line error. `Args.get` returns
`naught` for negative and out-of-range indices.

`Env.get` reads only the inherited process environment. It never searches for
or loads a `.env` file. A missing variable returns `naught`; a present variable
whose value is empty returns `""`; a non-UTF-8 value raises a recoverable
problem. `Env.read` explicitly reads a dotenv-like file. A missing file returns
`naught`, while permission, UTF-8, syntax, invalid-name, and duplicate-name
errors raise a problem. It accepts blank lines, `#` comments, optional
`export`, `NAME=VALUE`, and matching single or double quotes. It does not mutate
the process environment.

Path operations use the host operating system. `Path.list` returns full child
paths sorted by their string representation, so traversal order is
deterministic. I/O and non-UTF-8 path failures become recoverable Isen problems.

### `Json`

```text
Json.parse(value @@ string)             -> json
Json.stringify(value @@ json)           -> string
Json.pretty(value @@ json, spaces @@ int) -> string
Json.get(value @@ json, key @@ string)  -> perchance[json]
Json.at(value @@ json, index @@ int)    -> perchance[json]
Json.length(value @@ json)              -> int
Json.kind(value @@ json)                -> string
Json.as_string(value @@ json)           -> perchance[string]
Json.as_int(value @@ json)              -> perchance[int]
Json.as_float(value @@ json)            -> perchance[float]
Json.as_bool(value @@ json)             -> perchance[bool]
Json.is_null(value @@ json)             -> bool
Json.string(value @@ string)            -> json
Json.int(value @@ int)                  -> json
Json.float(value @@ float)              -> json
Json.bool(value @@ bool)                -> json
Json.null()                              -> json
Json.array(value @@ list[json])          -> json
Json.object(value @@ map[string, json])  -> json
Json.strings(value @@ map[string, string]) -> json
```

`json` is an opaque, heterogeneous value: it cannot be treated as an unchecked
map. Missing object keys and out-of-range array indices return `naught`; using
`get`, `at`, or `length` on the wrong JSON kind raises a problem. Scalar
extractors return `naught` on a kind mismatch. Pour a known-present
`perchance[json]` into `json` to unwrap it. Pretty indentation accepts 0 through
16 spaces; zero selects compact output. The constructor functions create JSON
without string concatenation; `Json.float` rejects non-finite values. Object
keys use Isen's deterministic map ordering.

### `Maths`

```text
Maths.exp(value @@ float)     -> float
Maths.log(value @@ float)     -> float
Maths.tanh(value @@ float)    -> float
Maths.sqrt(value @@ float)    -> float
Maths.sin(value @@ float)     -> float
Maths.cos(value @@ float)     -> float
Maths.abs(value @@ int|float) -> same type
Maths.floor(value @@ float)   -> int
Maths.min(a, b)               -> int or float; both must match
Maths.max(a, b)               -> int or float; both must match
Maths.pow(base, exponent)     -> float
```

`log` requires a positive value. `sqrt` requires a non-negative value.
Angles are in radians. The float constants `Maths.pi`, `Maths.tau`, `Maths.e`,
`Maths.phi`, `Maths.sqrt_two`, and `Maths.ln_two` are available after borrowing
`Maths`.

### `String`

```text
String.lower(value @@ string)                -> string
String.tokens(value @@ string)               -> list[string]
String.paragraph_tokens(value @@ string)     -> list[string]
String.slice(value @@ string, start, end)    -> string
String.split(value @@ string, separator)     -> list[string]
String.find(value @@ string, needle)         -> perchance[int]
String.join(values @@ list[string], separator) -> string
String.show(value)                              -> string
```

String indexing is character-based and zero-indexed: `"héllo"[1]` is `"é"`.
`slice` uses the half-open character range `[start, end)`. `find` also returns a
character index, or `naught`. Splitting on `""` yields individual characters.

`tokens` preserves alphanumeric words and apostrophes; every non-whitespace
symbol becomes its own token, so digits and punctuation no longer disappear.
`paragraph_tokens` additionally inserts `"<paragraph>"` between non-empty
paragraphs.

### `Input`

```text
Input.line(prompt @@ string) -> string
```

It prints and flushes the prompt, reads one line, and strips its line ending.
EOF returns `"/quit"`.

### `Keyboard`

```text
Keyboard.open()                    -> unit
Keyboard.key()                     -> string
Keyboard.read()                    -> perchance[string]
Keyboard.wait(milliseconds @@ int) -> perchance[string]
Keyboard.active()                  -> bool
Keyboard.close()                   -> unit
```

`Keyboard` is the opt-in Unix terminal interface for games and other live
programmes. `open` places interactive stdin in raw, non-echoing, nonblocking
mode. `key` blocks for one key, `read` returns immediately, and `wait` waits for
at most the non-negative timeout. Buffered keypresses are returned one at a
time. Ordinary Unicode characters return themselves; special values include
`up`, `down`, `left`, `right`, `home`, `end`, `delete`, `page_up`, `page_down`,
`enter`, `tab`, `backspace`, `escape`, and `ctrl_a` through `ctrl_z`.

Call `close` in an `always` block before continuing with line-oriented input.
Isen also invokes registered extension cleanup after every programme outcome,
including an uncaught failure, so the process cannot leave the terminal raw.
`Input.line` and `Keyboard` should not be used concurrently.

### `File`

```text
File.read(path @@ string)                 -> string
File.write(path @@ string, value @@ string) -> unit
File.append(path @@ string, value @@ string) -> unit
File.lines(path @@ string)                -> list[string]
File.make_dir(path @@ string)             -> unit
File.text_files(directory @@ string)      -> list[string]
```

`File.write` writes a sibling `.tmp` file and renames it over the destination.
`File.append` creates the file when absent and appends the supplied bytes in a
single open/write operation; it does not add a newline.
`text_files` returns sorted direct children whose extension is `.txt`, ignoring
case; it does not recurse.

### `List`

```text
List.push(values @@ list[T], value @@ T) -> list[T]
List.append(values @@ list[T], value @@ T) -> unit
List.pop(values @@ list[T])                   -> perchance[T]
List.shift(values @@ list[T])                 -> perchance[T]
```

`push` is functional. The other three operations mutate the passed list.

`Stack.push`/`Stack.pop` and `Queue.push`/`Queue.pop` expose the same mutable
list storage with explicit LIFO and FIFO vocabulary.

### `Stack` and `Queue`

```text
Stack.push(values @@ list[T], value @@ T) -> unit
Stack.pop(values @@ list[T])              -> perchance[T]
Queue.push(values @@ list[T], value @@ T) -> unit
Queue.pop(values @@ list[T])              -> perchance[T]
```

Both spaces mutate the supplied list. `Stack.pop` removes the last value
(LIFO); `Queue.pop` removes the first value (FIFO). Empty structures return
`naught`.

### `Test`

```text
Test.assert(condition @@ bool, message @@ string)                 -> unit
Test.equal(actual @@ T, expected @@ T, message @@ string)         -> unit
Test.not_equal(actual @@ T, unexpected @@ T, message @@ string)   -> unit
Test.fail(message @@ string)                                      -> unit
```

Every failed assertion raises a recoverable `Problem`. `equal` and `not_equal`
use the same structural comparison implementation as language equality.

### `Range` and `Ordering`

```text
Range.until(stop @@ int)                         -> list[int]
Range.between(start @@ int, stop @@ int)         -> list[int]
Range.step(start @@ int, stop @@ int, step @@ int) -> list[int]
Ordering.less(left @@ ordered, right @@ same)     -> bool
Ordering.compare(left @@ ordered, right @@ same)  -> int
```

Ranges are half-open. A step may be positive or negative but not zero.
`ordered` means `int`, `float`, or `string`; both arguments must have the same
type. Other types are rejected by the checker. `compare` returns -1, 0, or 1.

### `Map`

```text
Map.string_int()                                      -> map[string, int]
Map.has(values @@ map[K, V], key @@ K)              -> bool
Map.get(values @@ map[K, V], key @@ K)              -> perchance[V]
Map.keys(values @@ map[K, V])                       -> list[K]
Map.top_string_int(values @@ map[string, int], limit)   -> list[string]
```

`top_string_int` sorts by descending count and then ascending word. Its limit
must be a non-negative `int`.

### `Array`

General constructors and numeric operations:

```text
Array.float(length @@ int, initial @@ float) -> arr[float]
Array.int(length @@ int, initial @@ int)     -> arr[int]
Array.sized(length @@ int, initial @@ T)     -> arr[T]
Array.dot(a, a_offset, b, b_offset, length)  -> float
Array.axpy(target, target_offset, source, source_offset, length, scale) -> unit
Array.fill(target, offset, length, value)    -> unit
Array.copy(target, target_offset, source, source_offset, length)        -> unit
Array.save(path @@ string, values @@ arr[float]) -> unit
Array.load_float(path @@ string)                 -> arr[float]
```

Offsets and lengths are non-negative and bounds-checked.

### Optional `ML` extension

The specialised dense-model kernels are excluded from the default interpreter.
Build them explicitly with `cargo build --release --features ml-kernels`, then
`borrow ML`. This adds:

```text
ML.mlp_forward
ML.sampled_update
ML.sampled_softmax_update
ML.mlp_backprop
ML.softmax_sample
ML.gru_forward
ML.gru_backprop
```

These are specialised numerical intrinsics rather than a stable general
library interface. Use `examples/mlp_lm.is` and `examples/gru_lm.is` as their
current calling contract.

### `LengText`

```text
LengText.red(value)       -> string
LengText.yellow(value)    -> string
LengText.green(value)     -> string
LengText.blue(value)      -> string
LengText.indent(value @@ string, spaces @@ int) -> string
LengText.pretty_json(value @@ json, spaces @@ int) -> string
LengText.flush()          -> unit
LengText.blit(frame @@ string) -> unit
LengText.screen_begin()   -> unit
LengText.screen_end()     -> unit
LengText.size()           -> list[int]
```

Each accepts exactly one displayable value and wraps its rendered form in ANSI
colour codes. `indent` prefixes every non-empty line and accepts 0 through 64
spaces. `pretty_json` uses the same canonical JSON formatter as `Json.pretty`.
`flush` clears the terminal. `screen_begin` clears once and hides the cursor;
`blit` performs one cursor-home/frame/erase-tail write; `screen_end` restores
the cursor. `size` returns `[columns, rows]` from the current terminal when
available. Call `screen_end` from `always` when animation can fail. Isen also
restores the cursor through registered extension cleanup after every programme
outcome, including an uncaught failure.

### `Bytes`, `Udp`, `Tcp`, and `Http`

These spaces are independent opt-in capabilities. `arr[int]` is the byte
container; every native boundary rejects elements outside 0 through 255.

`Bytes.encode(string)` produces UTF-8 bytes. `Bytes.decode(bytes)` returns
`perchance[string]`, using `naught` for invalid UTF-8.

`Udp` accepts hostnames plus IPv4 and IPv6 addresses. It supports bound and
connected sockets, text and binary sends, bounded receive buffers, readiness,
broadcast, nonblocking mode, and local-address inspection. `Udp.receive`
returns `udp_packet`; `.bytes` is lossless and `.text` is present only for
UTF-8 payloads.

`Tcp` supplies listeners and streams. Connect has an explicit millisecond
timeout. `accept` blocks; `try_accept` returns `naught` on a nonblocking
listener when no connection is queued. Reads are bounded and return bytes;
`read_text` returns `naught` for non-UTF-8. Empty bytes indicate orderly EOF.
Writes are available in partial and `write_all` forms. Shutdown modes are
`"read"`, `"write"`, and `"both"`.

`Http.get` and `Http.request` perform bounded plain HTTP/1.1 exchanges.
`Http.request` accepts method, host, port, target, `map[string, string]`
headers, body bytes, timeout, and response-byte limit. The result is an
`http_response`: status is `int`, reason/version are `string`, headers are a
lowercase `map[string, string]`, body is `arr[int]`, and text is
`perchance[string]`. Chunked responses are decoded. Redirects, cookies,
authentication, decompression, pooling, and application policy belong in Isen
stashes. HTTPS/TLS is not currently provided. `Http` never imports `Json`.

## Generated native API catalog

<!-- BEGIN GENERATED:NATIVE_API -->
This catalog is generated from the native extension registry. Argument names are
positional (`arg1`, `arg2`, …); package guidance above provides descriptive names.
Angle-bracketed constraints and `T1`/`K1`/`V1` are documentation metavariables,
not source-level type names.

### `Args` generated surface

```text
Args.all() -> list[string]
Args.get(arg1 @@ int) -> perchance[string]
```

### `Array` generated surface

```text
Array.float(arg1 @@ int, arg2 @@ float) -> arr[float]
Array.int(arg1 @@ int, arg2 @@ int) -> arr[int]
Array.sized(arg1 @@ int, arg2 @@ <any value>) -> arr[type_of_arg2]
Array.dot(arg1 @@ arr[float], arg2 @@ int, arg3 @@ arr[float], arg4 @@ int, arg5 @@ int) -> float
Array.axpy(arg1 @@ arr[float], arg2 @@ int, arg3 @@ arr[float], arg4 @@ int, arg5 @@ int, arg6 @@ float) -> unit
Array.fill(arg1 @@ arr[float], arg2 @@ int, arg3 @@ int, arg4 @@ float) -> unit
Array.copy(arg1 @@ arr[float], arg2 @@ int, arg3 @@ arr[float], arg4 @@ int, arg5 @@ int) -> unit
Array.save(arg1 @@ string, arg2 @@ arr[float]) -> unit
Array.load_float(arg1 @@ string) -> arr[float]
```

### `Bytes` generated surface

```text
Bytes.encode(arg1 @@ string) -> arr[int]
Bytes.decode(arg1 @@ arr[int]) -> perchance[string]
```

### `Env` generated surface

```text
Env.get(arg1 @@ string) -> perchance[string]
Env.read(arg1 @@ string) -> perchance[map[string, string]]
```

### `File` generated surface

```text
File.read(arg1 @@ string) -> string
File.write(arg1 @@ string, arg2 @@ string) -> unit
File.append(arg1 @@ string, arg2 @@ string) -> unit
File.lines(arg1 @@ string) -> list[string]
File.make_dir(arg1 @@ string) -> unit
File.text_files(arg1 @@ string) -> list[string]
```

### `Http` generated surface

```text
Http.get(arg1 @@ string, arg2 @@ int, arg3 @@ string, arg4 @@ map[string, string], arg5 @@ int, arg6 @@ int) -> http_response
Http.request(arg1 @@ string, arg2 @@ string, arg3 @@ int, arg4 @@ string, arg5 @@ map[string, string], arg6 @@ arr[int], arg7 @@ int, arg8 @@ int) -> http_response
```

### `Input` generated surface

```text
Input.line(arg1 @@ string) -> string
```

### `Json` generated surface

```text
Json.parse(arg1 @@ string) -> json
Json.stringify(arg1 @@ json) -> string
Json.pretty(arg1 @@ json, arg2 @@ int) -> string
Json.get(arg1 @@ json, arg2 @@ string) -> perchance[json]
Json.at(arg1 @@ json, arg2 @@ int) -> perchance[json]
Json.length(arg1 @@ json) -> int
Json.kind(arg1 @@ json) -> string
Json.as_string(arg1 @@ json) -> perchance[string]
Json.as_int(arg1 @@ json) -> perchance[int]
Json.as_float(arg1 @@ json) -> perchance[float]
Json.as_bool(arg1 @@ json) -> perchance[bool]
Json.is_null(arg1 @@ json) -> bool
Json.string(arg1 @@ string) -> json
Json.int(arg1 @@ int) -> json
Json.float(arg1 @@ float) -> json
Json.bool(arg1 @@ bool) -> json
Json.null() -> json
Json.array(arg1 @@ list[json]) -> json
Json.object(arg1 @@ map[string, json]) -> json
Json.strings(arg1 @@ map[string, string]) -> json
```

### `Keyboard` generated surface

```text
Keyboard.open() -> unit
Keyboard.read() -> perchance[string]
Keyboard.key() -> string
Keyboard.wait(arg1 @@ int) -> perchance[string]
Keyboard.close() -> unit
Keyboard.active() -> bool
```

### `Kwargs` generated surface

```text
Kwargs.all() -> map[string, string]
Kwargs.get(arg1 @@ string) -> perchance[string]
Kwargs.has(arg1 @@ string) -> bool
```

### `LengText` generated surface

```text
LengText.red(arg1 @@ <any value>) -> string
LengText.yellow(arg1 @@ <any value>) -> string
LengText.green(arg1 @@ <any value>) -> string
LengText.blue(arg1 @@ <any value>) -> string
LengText.black(arg1 @@ <any value>) -> string
LengText.magenta(arg1 @@ <any value>) -> string
LengText.cyan(arg1 @@ <any value>) -> string
LengText.white(arg1 @@ <any value>) -> string
LengText.grey(arg1 @@ <any value>) -> string
LengText.bright_red(arg1 @@ <any value>) -> string
LengText.bright_yellow(arg1 @@ <any value>) -> string
LengText.bright_green(arg1 @@ <any value>) -> string
LengText.bright_blue(arg1 @@ <any value>) -> string
LengText.bright_magenta(arg1 @@ <any value>) -> string
LengText.bright_cyan(arg1 @@ <any value>) -> string
LengText.bright_white(arg1 @@ <any value>) -> string
LengText.orange(arg1 @@ <any value>) -> string
LengText.pink(arg1 @@ <any value>) -> string
LengText.purple(arg1 @@ <any value>) -> string
LengText.violet(arg1 @@ <any value>) -> string
LengText.teal(arg1 @@ <any value>) -> string
LengText.lime(arg1 @@ <any value>) -> string
LengText.gold(arg1 @@ <any value>) -> string
LengText.sky(arg1 @@ <any value>) -> string
LengText.flush() -> unit
LengText.blit(arg1 @@ string) -> unit
LengText.screen_begin() -> unit
LengText.screen_end() -> unit
LengText.size() -> list[int]
LengText.palette() -> unit
LengText.indent(arg1 @@ string, arg2 @@ int) -> string
LengText.pretty_json(arg1 @@ json, arg2 @@ int) -> string
```

### `List` generated surface

```text
List.push(arg1 @@ list[T1], arg2 @@ <any value>) -> <same type as arg1>
List.append(arg1 @@ list[T1], arg2 @@ <any value>) -> unit
List.pop(arg1 @@ list[T1]) -> perchance[element_of_arg1]
List.shift(arg1 @@ list[T1]) -> perchance[element_of_arg1]
```

### `Map` generated surface

```text
Map.string_int() -> map[string, int]
Map.has(arg1 @@ map[K1, V1], arg2 @@ <any value>) -> bool
Map.get(arg1 @@ map[K1, V1], arg2 @@ <any value>) -> perchance[value_of_arg1]
Map.keys(arg1 @@ map[K1, V1]) -> list[key_of_arg1]
Map.top_string_int(arg1 @@ map[string, int], arg2 @@ int) -> list[string]
```

### `Maths` generated surface

```text
Maths.exp(arg1 @@ float) -> float
Maths.log(arg1 @@ float) -> float
Maths.tanh(arg1 @@ float) -> float
Maths.sqrt(arg1 @@ float) -> float
Maths.sin(arg1 @@ float) -> float
Maths.cos(arg1 @@ float) -> float
Maths.abs(arg1 @@ <int or float>) -> <same type as arg1>
Maths.floor(arg1 @@ float) -> int
Maths.min(arg1 @@ <int or float>, arg2 @@ <same type as arg1>) -> <same type as arg1>
Maths.max(arg1 @@ <int or float>, arg2 @@ <same type as arg1>) -> <same type as arg1>
Maths.pow(arg1 @@ float, arg2 @@ float) -> float
Maths.pi @@ float
Maths.tau @@ float
Maths.e @@ float
Maths.phi @@ float
Maths.sqrt_two @@ float
Maths.ln_two @@ float
```

### `Ordering` generated surface

```text
Ordering.less(arg1 @@ <int, float, or string>, arg2 @@ <same type as arg1>) -> bool
Ordering.compare(arg1 @@ <int, float, or string>, arg2 @@ <same type as arg1>) -> int
```

### `Path` generated surface

```text
Path.current() -> string
Path.join(arg1 @@ string, arg2 @@ string) -> string
Path.exists(arg1 @@ string) -> bool
Path.is_file(arg1 @@ string) -> bool
Path.is_dir(arg1 @@ string) -> bool
Path.canonical(arg1 @@ string) -> string
Path.list(arg1 @@ string) -> list[string]
Path.name(arg1 @@ string) -> perchance[string]
Path.parent(arg1 @@ string) -> perchance[string]
```

### `Queue` generated surface

```text
Queue.push(arg1 @@ list[T1], arg2 @@ <any value>) -> unit
Queue.pop(arg1 @@ list[T1]) -> perchance[element_of_arg1]
```

### `Random` generated surface

```text
Random.int(arg1 @@ int, arg2 @@ int) -> int
Random.float(arg1 @@ float, arg2 @@ float) -> float
Random.seed(arg1 @@ int) -> unit
```

### `Range` generated surface

```text
Range.until(arg1 @@ int) -> list[int]
Range.between(arg1 @@ int, arg2 @@ int) -> list[int]
Range.step(arg1 @@ int, arg2 @@ int, arg3 @@ int) -> list[int]
```

### `Stack` generated surface

```text
Stack.push(arg1 @@ list[T1], arg2 @@ <any value>) -> unit
Stack.pop(arg1 @@ list[T1]) -> perchance[element_of_arg1]
```

### `String` generated surface

```text
String.tokens(arg1 @@ string) -> list[string]
String.paragraph_tokens(arg1 @@ string) -> list[string]
String.lower(arg1 @@ string) -> string
String.slice(arg1 @@ string, arg2 @@ int, arg3 @@ int) -> string
String.split(arg1 @@ string, arg2 @@ string) -> list[string]
String.find(arg1 @@ string, arg2 @@ string) -> perchance[int]
String.join(arg1 @@ list[string], arg2 @@ string) -> string
String.show(arg1 @@ <any value>) -> string
```

### `Tcp` generated surface

```text
Tcp.listen(arg1 @@ string, arg2 @@ int) -> tcp_listener
Tcp.connect(arg1 @@ string, arg2 @@ int, arg3 @@ int) -> tcp_stream
Tcp.accept(arg1 @@ tcp_listener) -> tcp_stream
Tcp.try_accept(arg1 @@ tcp_listener) -> perchance[tcp_stream]
Tcp.read(arg1 @@ tcp_stream, arg2 @@ int) -> arr[int]
Tcp.read_text(arg1 @@ tcp_stream, arg2 @@ int) -> perchance[string]
Tcp.ready(arg1 @@ tcp_stream, arg2 @@ int) -> bool
Tcp.write(arg1 @@ tcp_stream, arg2 @@ string) -> int
Tcp.write_bytes(arg1 @@ tcp_stream, arg2 @@ arr[int]) -> int
Tcp.write_all(arg1 @@ tcp_stream, arg2 @@ string) -> unit
Tcp.write_all_bytes(arg1 @@ tcp_stream, arg2 @@ arr[int]) -> unit
Tcp.shutdown(arg1 @@ tcp_stream, arg2 @@ string) -> unit
Tcp.set_nonblocking(arg1 @@ tcp_stream, arg2 @@ bool) -> unit
Tcp.set_listener_nonblocking(arg1 @@ tcp_listener, arg2 @@ bool) -> unit
Tcp.set_nodelay(arg1 @@ tcp_stream, arg2 @@ bool) -> unit
Tcp.local_host(arg1 @@ tcp_stream) -> string
Tcp.local_port(arg1 @@ tcp_stream) -> int
Tcp.peer_host(arg1 @@ tcp_stream) -> string
Tcp.peer_port(arg1 @@ tcp_stream) -> int
Tcp.listener_host(arg1 @@ tcp_listener) -> string
Tcp.listener_port(arg1 @@ tcp_listener) -> int
```

### `Test` generated surface

```text
Test.assert(arg1 @@ bool, arg2 @@ string) -> unit
Test.equal(arg1 @@ <any value>, arg2 @@ <same type as arg1>, arg3 @@ string) -> unit
Test.not_equal(arg1 @@ <any value>, arg2 @@ <same type as arg1>, arg3 @@ string) -> unit
Test.fail(arg1 @@ string) -> unit
```

### `Time` generated surface

```text
Time.clock() -> int
Time.sleep(arg1 @@ int) -> unit
Time.since(arg1 @@ int) -> int
Time.unix_millis() -> int
Time.utc() -> string
```

### `Udp` generated surface

```text
Udp.bind(arg1 @@ string, arg2 @@ int) -> udp_socket
Udp.connect(arg1 @@ udp_socket, arg2 @@ string, arg3 @@ int) -> unit
Udp.send(arg1 @@ udp_socket, arg2 @@ string) -> int
Udp.send_bytes(arg1 @@ udp_socket, arg2 @@ arr[int]) -> int
Udp.send_to(arg1 @@ udp_socket, arg2 @@ string, arg3 @@ int, arg4 @@ string) -> int
Udp.send_bytes_to(arg1 @@ udp_socket, arg2 @@ string, arg3 @@ int, arg4 @@ arr[int]) -> int
Udp.receive(arg1 @@ udp_socket, arg2 @@ int) -> udp_packet
Udp.ready(arg1 @@ udp_socket, arg2 @@ int) -> bool
Udp.local_host(arg1 @@ udp_socket) -> string
Udp.local_port(arg1 @@ udp_socket) -> int
Udp.set_broadcast(arg1 @@ udp_socket, arg2 @@ bool) -> unit
Udp.set_nonblocking(arg1 @@ udp_socket, arg2 @@ bool) -> unit
```
<!-- END GENERATED:NATIVE_API -->

## Common generation mistakes

Do not generate these forms:

| Wrong | Correct |
| --- | --- |
| `import Maths` or `steal Maths` | `borrow Maths` |
| `while ready { ... }` | `aslongas ready $ ... \$` |
| `for item in items` | `each item in items $ ... \$` |
| `func f()` | `given f() @@ ReturnType $ ... \$` |
| `return value` | `ret value` |
| `break` | `enough` |
| `continue` | `onwards` |
| `struct User` or `mold User` | `form User $ ... \$` |
| `print(value)` | `say(value)` |
| `value.into(string)` | `value.pour_into(string)` |
| `{ "key": value }` | `#{ "key": value }` |
| `exit()` | `exit` |
| use `Maths` without setup | place `borrow Maths` before first use |

Other important traps:

- Empty `[]`, `@[]`, and `#{}` require an expected collection type from an
  annotation, function parameter, return type, form field, or assignment.
- List, array, and map slots and form fields are assignable; nested assignment
  is valid when the final container or form is mutable.
- Do not mix `int` and `float` arithmetic without `.pour_into(...)`.
- Direct indexing of a missing map key raises; use `Map.get` for `naught`.
- Do not assume borrowings are global. `borrow` is lexical.

## Interpreter architecture appendix

The interpreter is intentionally compact and currently lives primarily in
`src/main.rs`:

1. `lex` converts source string into line-spanned tokens.
2. `Parser` creates `Stmt` and `Expr` trees with a Pratt expression parser.
3. `Ty`, `Value`, and `Data` represent runtime types and values.
4. `Env` is an `Rc<RefCell<...>>` lexical environment containing values,
   form definitions, lazy shipped-package descriptors, and a parent pointer.
5. `run` and `exec` interpret statements. `eval` interprets expressions.
6. `Flow` propagates `ret`, `enough`, and `onwards`. `exit` uses a clean
   exit signal carried through the normal error channel.
7. `size` is the only direct core builtin. Every opt-in shipped
   space uses the generic native callback or runtime callback in `src/native.rs`.
8. `build.rs` automatically discovers self-registering Rust files in
   `src/extensions/`; adding one needs no parser, builtin enum, or dispatch
   edits. Runtime registrars and namespace scopes are invoked lazily by
   `borrow`; unborrowed extensions add no scopes. See
   [extension authoring](EXTENSIONS.md).
9. Shipped implementations live in `src/extensions/`; networking capabilities
   are isolated in `network.rs`, while feature-gated `ML` kernels do not live
   in the interpreter core or default runtime surface.

Diagnostics contain a canonical source filename, line number, and message when
checking or running files. Errors inside borrowed stashes retain the stash's
source filename. Columns and full call stacks are not yet tracked.

After parsing, the checker resolves lexical names and the complete borrowed
stash graph, then validates all branches and function bodies before execution.
It checks declarations, assignments, arguments, returns, operators, conditions,
containers, form construction and fields, namespace members, indexing, casts,
and loop control. Non-unit functions that can reach their closing `\$` without
`ret`, `scream`, `raise`, or `exit` are rejected. Runtime checks remain for value-dependent
failures such as bounds, absent map keys, fallible casts, malformed files, I/O,
sockets, and defence against faulty native extensions.

## Stash implementation notes

Local libraries use explicit `share`/`borrow` rather than textual inclusion:

```is
// lib/temperature.is
given celsius_to_fahrenheit(value @@ float) @@ float $
  ret value * 1.8 + 32.0
\$
share celsius_to_fahrenheit

// app.is
borrow celsius_to_fahrenheit from "lib/temperature.is"
say(celsius_to_fahrenheit(20.0))
```

`share name` must appear at stash top level after the named `dec`, `given`,
`space`, or `form` exists. A stash runs in an isolated scope parented by the
core runtime, so it must perform its own shipped-space borrowings. Shared
functions retain that scope as their closure. Shared form definitions are
installed into the borrowing scope.

Paths resolve relative to the file containing the `borrow`, then canonicalise.
The canonical path is both module identity and the cache key, so borrowing more
than one name does not rerun top-level code. An active-load stack rejects cycles
and reports their path chain. There is deliberately no wildcard borrowing form.

## Verification commands

```sh
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
./isen --reference --check
./isen examples/tour.is
./isen examples/showcase.is
./isen examples/donut/donut.is
```

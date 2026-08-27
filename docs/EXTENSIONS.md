# Rust-backed Isen extensions

Isen programmes remain interpreted. An extension only supplies a small
native operation where filesystem access, platform integration, or a tight
numeric loop is better expressed in Rust. Isen should still own the
orchestration and application logic.

## Adding a space

Add one Rust file to `src/extensions/`. The build discovers every `.rs` file in
that directory, so there is no runtime module list, central builtin enum, or
interpreter match statement to edit. Filenames may contain letters, digits, and
underscores. Runtime callbacks and checker-visible signatures live together in
that extension file; the registry verifies their names and order agree.

This complete extension defines `Demo.twice`:

```rust
use crate::native::{NativeCall, NativeFunction, NativeRegistry,
    NativeSignature as Signature, NativeSpace};
use crate::{Result, Ty};

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add(NativeSpace {
        name: "Demo",
        functions: &[NativeFunction {
            name: "twice",
            call: twice,
        }],
        signatures: || vec![
            Signature::exact("twice", vec![Ty::Int], Ty::Int),
        ],
    });
}

fn twice(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(1, "Demo.twice")?;
    Ok(call.int_value(call.int(0, "Demo.twice")? * 2))
}
```

After rebuilding, Isen opts into it exactly like every shipped space:

```is
borrow Demo
say(Demo.twice(21))
```

The usual shape for a substantial library is a thin Rust space plus an ordinary
Isen stash. The stash borrows the primitive, implements policy in the
language, and shares its friendly public API:

```is
// lib/doubling.is
borrow Demo

given doubled_label(value @@ int) @@ string $
  ret Demo.twice(value).pour_into(string) + " things"
\$
share doubled_label
```

Consumers borrow `doubled_label` from the stash and never need to know that its
small hot operation is native. A native dependency beyond Rust's standard
library must still be declared in `Cargo.toml`.

## Native API

`NativeCall` contains already evaluated Isen arguments and the calling
source line. Its typed readers are `int`, `float`, `bool`, and `string`; `shown`
uses Isen's normal display representation. `exactly` checks arity and
`error` creates a source-aware runtime error.

Return values use `int_value`, `float_value`, `bool_value`, `string_value`,
`json_value`, or `unit_value`. This deliberately narrow surface prevents an extension from
depending on the parser or AST. Extensions that operate on specialised arrays,
forms, or network resources may currently use the crate-internal `Value` and
`Data` representation, but reusable typed wrappers should be added to
`src/native.rs` instead of duplicating fragile extraction code.

Spaces may also expose typed values with `NativeRegistry::add_constant` after
registering the space. Integer, float, boolean, and string constants are
supported; `Maths.pi` and `Maths.e` are representative examples.

`Maths`, `LengText`, and `Time` are small reference implementations. The standard
spaces demonstrate files, collections, input, string, and randomness;
`network.rs` keeps `Bytes`, `Udp`, `Tcp`, and `Http` isolated from those general
facilities. `array.rs` demonstrates the lower-level runtime callback reserved for
operations that need mutable interpreter containers. New
extensions should prefer the evaluated `NativeCall` API unless that deeper
access is genuinely necessary.

`ml.rs` is an optional extension registry enabled by Cargo's `ml-kernels`
feature. Its implementations are compiled out entirely when the feature is
absent, and its functions live under `ML` rather than the general `Array`
space.

The metadata-only registry also generates the authoritative native API catalog
in [LANGUAGE_REFERENCE.md](LANGUAGE_REFERENCE.md). After changing a signature or constant, run
`./isen --reference`; CI and `scripts/release-check.sh` use
`./isen --reference --check` to reject stale generated documentation.

## Design rules

- A native extension exposes an opt-in space; it does not silently add globals.
- Runtime registration is lazy. The generated registry retains module loaders,
  and `borrow` invokes loaders until it discovers the requested package. The
  namespace handle and scope are materialized only for that borrowed package.
- Extensions that acquire process resources may register a cleanup callback.
  Isen runs callbacks in reverse registration order after success, `exit`, or
  failure; the `Keyboard` extension uses this to restore terminal attributes.
- Perform argument validation before work and return an Isen `Error`
  rather than panicking on user input.
- Keep policy, loops, and application behaviour in `.is` files. Rust is for a
  primitive operation, not a disguised second implementation of the library.
- Space names must be unique. Duplicate registration is a build-time developer
  mistake and deliberately panics while constructing the runtime.
- Adding or removing an extension requires rebuilding `isen`; using an already
  compiled extension only requires `borrow SpaceName`.

The discovery mechanism lives in `build.rs`, the stable-ish authoring surface in
`src/native.rs`, and extension implementations in `src/extensions/`.

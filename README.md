# lightningcss-compact

A Rust library and command-line compiler that compacts ordinary CSS and its
class bindings at build time. Author readable CSS, then emit shorter class names
and reuse declarations where the compiler can preserve the cascade. The browser
receives CSS and bound class strings; it needs no stylesheet generator or runtime
class lookup.

This is a separate project from any template framework. HTML, Rust templates,
and JavaScript build tools can consume the same generated manifest through their
own adapters. JavaScript bindings describe class usage, not CSS-in-JS styling.

## What it optimizes

The compiler parses CSS with Lightning CSS, inventories class usage across a
project, and plans transformations before writing any stylesheet. It can shorten
owned class identifiers and factor eligible repeated declarations into shared
classes. It keeps residual rules when conditions, specificity, declaration
conflicts, or loading order prevent safe factoring.

For example, static bindings can expand an authored `card` class into a short
identity class plus shared declaration classes. Selectors use the identity class;
dynamic class tokens stay single tokens. Removing a state class must not leave
its styling attached through an expanded atom list.

This is a conservative optimization, not a claim of globally minimal CSS.
Atomization can increase HTML size. Compare compressed CSS, rendered HTML, and
affected scripts for the routes you serve, alongside build time and browser
behavior. The report records transformations and skipped opportunities.

## Requirements and installation

The minimum supported Rust version is 1.88.0. The direct AST adapter is pinned to
Lightning CSS `1.0.0-alpha.72`; matching upstream types matter because that API is
pre-release.

For the library:

```toml
[dependencies]
lightningcss-compact = "0.1"
# Add serde_json = "1" if your integration reads a JSON inventory.
```

For the optional CLI, after a crate release is available:

```sh
cargo install lightningcss-compact --features cli --locked
lightningcss-compact --help
```

To build the CLI from this checkout:

```sh
cargo build --release --features cli --locked
cargo run --features cli -- --help
```

Tagged GitHub releases provide CLI archives for Linux, macOS, and Windows,
with checksums. Their targets are listed in the release notes; unlisted systems
can build from source. Linux GNU archives are built on Ubuntu 24.04 and require a
compatible glibc runtime. Publishing to crates.io is a separate release action.

## Project inputs and bindings

A project inventory supplies CSS inputs and ordered loading groups, literal
bindings, and managed, dynamic, and reserved class names. Class renaming and
expansion require complete usage information. External identifiers and classes
created by libraries must be preserved or explicitly accounted for. A CSS-only
inventory cannot establish how HTML or scripts use its classes.
Generated naming requires case-sensitive class matching in standards-mode
documents (`<!doctype html>`). Quirks-mode HTML is outside this release's
renaming and expansion contract; use name-preserving output there.

Bindings have four purposes:

| Kind | Consumer output |
| --- | --- |
| `classes` | Static class list, including eligible shared declaration classes |
| `selector` | Parsed selector with owned identity classes renamed |
| `token` | One class token for dynamic assignment, removal, or membership checks |
| `html` | HTML fragment with actual class attributes bound, without rewriting text or code |

The Rust entrypoints are `prepare_project(input, options)`,
`apply_plan(&prepared, sheet_id, &mut stylesheet)`, and
`compile_project(input, options)`. The library re-exports `lightningcss` so the
native visitor adapter can use its matching AST types. Preparation fixes one
mapping for the project. `apply_plan` refuses a stylesheet whose canonical AST
differs from the prepared input; run it before other transforms change that AST.
The applied rules are already minified; print them with `PrinterOptions`
instead of running the upstream minifier again. Version alpha.72 needs a
temporary rule-boundary guard against unstable deduplication; see the
[minifier compatibility note](docs/ALGORITHM.md#minifier-compatibility).
Compiled output includes CSS, bound literals, source maps, a versioned manifest,
and a report. Template adapters consume these outputs during compilation.

```rust
use lightningcss_compact::{compile_project, Options, ProjectInput};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string("examples/project.json")?;
    let input: ProjectInput = serde_json::from_str(&source)?;
    let compiled = compile_project(input, Options::default())?;
    println!("{}", compiled.stylesheets["cards.css"]);
    println!("{}", compiled.bindings["document"]);
    Ok(())
}
```

See the executable [Rust example](examples/bundle.rs) and
[input example](examples/project.json). The JSON embeds CSS in each stylesheet's
`source` field; it does not load stylesheet paths. Rust consumers can construct
the public structs directly without using JSON.

Without a complete inventory, preserve class identifiers and use CSS-only
analysis/minification. Do not expand or rename classes based on CSS alone.
Unsupported transformations retain their original semantics; invalid bindings
and inconsistent inventories produce errors.

## CLI workflow

```sh
lightningcss-compact analyze --project examples/project.json
lightningcss-compact build --project examples/project.json --out-dir dist/build
lightningcss-compact check --project examples/project.json
lightningcss-compact eval --project examples/project.json --out-dir dist/evaluation
```

`analyze` prints the report without writing files. `build` emits `css/<id>`,
adjacent `.map` files, `bindings.json`, `manifest.json`, `report.json`, and
`result.json`. The default command is `build`. `check` validates the plan and
asserts its reported Brotli and gzip totals do not exceed the guarded Lightning CSS control;
it does not run browser equivalence tests. `eval` writes separate `baseline`,
`naming`, and `compact` outputs plus `comparison.json`. Use `--mode baseline`,
`--mode naming`, or `--mode compact` with the other commands. Search bounds are
library `Options`, not CLI flags.

Reports compress each CSS stylesheet and each HTML binding separately. Other
bound literals form one deterministic synthetic JavaScript/template stream.
This measures expanded class lists but is not your application's final bundle
cost. Supply complete rendered HTML streams where possible, then separately
measure actual served HTML, CSS, and scripts. Encoder settings and the precise
finite search claims are documented in [the algorithm](docs/ALGORITHM.md).

Keep authored CSS, generated CSS, and consumer bindings separate. Never replace
class strings with a regular-expression pass over template source, scripts,
Markdown examples, or arbitrary HTML text. A template adapter must use its
language's syntax and report bindings it cannot safely transform.

## Cascade and deployment constraints

Class attribute order does not determine CSS precedence. The compiler preserves
loading order, layers, conditions, importance, specificity, and conflicts between
properties, including shorthand relationships. Shared declarations cannot move
across conflicting writes just because their text is identical. Residual complex
selectors can still use shortened identity classes.

Serve compiled CSS and its bound markup as one manifest generation. Retained
documents, soft navigation, and browser/CDN caches can mix releases unless the
application checks generation compatibility. Content-addressed asset URLs must
resolve to their exact bytes. Keep older assets available for cached documents,
or implement an explicit document-refresh recovery path.

Rust and the CLI are the first release surface. An npm/Node visitor adapter is
deferred; this project currently ships no npm package or runtime JavaScript
styling API.

## Development and license

See [CONTRIBUTING.md](CONTRIBUTING.md) for checks and release validation.
Original code is licensed under [MIT](LICENSE-MIT) OR
[Apache-2.0](LICENSE-APACHE), at your option. Dependencies retain their licenses;
see [NOTICE](NOTICE), [dependency declarations](THIRD-PARTY.md), and the
[copied license and copyright texts](licenses/third-party/README.md).

# Synthetic evaluation

The compiler preserves the authored rendering in all **31 fixtures and 89 native Chrome states**, and the harness detects all **89 deliberately incorrect variants**. Across this synthetic corpus, compact output reduces the independently compressed CSS, HTML and binding streams by **4.41% Brotli bytes against stock Lightning CSS alpha.72**, or **4.65% against the guarded baseline**. These are corpus results, not a forecast for another application.

The stock control is a separate executable that calls the upstream parser, default minifier and printer without the compact plugin. The guarded baseline uses the same upstream version with temporary rule boundaries to prevent a demonstrated upstream nondeterminism defect. Naming adds identity shortening; compact also searches safe declaration and bundle sharing. Across this corpus, the guarded baseline is 13 Brotli bytes larger than stock. Baseline selector-binding printing also changes the affected JavaScript stream; the breakdown below exposes each contribution.

Combined CSS, HTML and affected JavaScript binding-stream totals:

| Output | Raw bytes | Brotli bytes | Gzip bytes |
| --- | ---: | ---: | ---: |
| Stock alpha.72 | 42,199 | 5,240 | 7,371 |
| Guarded baseline | 42,639 | 5,253 | 7,392 |
| Naming | 40,206 | 5,100 | 7,182 |
| Compact | 39,796 | 5,009 | 7,092 |

The same measurements separated by stream:

| Stream | Output | Raw bytes | Brotli bytes | Gzip bytes |
| --- | --- | ---: | ---: | ---: |
| CSS | Stock alpha.72 | 2,809 | 1,897 | 2,636 |
| CSS | Guarded baseline | 3,266 | 1,917 | 2,666 |
| CSS | Naming | 2,957 | 1,830 | 2,480 |
| CSS | Compact | 2,546 | 1,760 | 2,416 |
| HTML | Stock alpha.72 | 39,191 | 3,157 | 4,501 |
| HTML | Guarded baseline | 39,191 | 3,157 | 4,501 |
| HTML | Naming | 37,100 | 3,124 | 4,507 |
| HTML | Compact | 37,101 | 3,103 | 4,481 |
| Affected JS bindings | Stock alpha.72 | 199 | 186 | 234 |
| Affected JS bindings | Guarded baseline | 182 | 179 | 225 |
| Affected JS bindings | Naming | 149 | 146 | 195 |
| Affected JS bindings | Compact | 149 | 146 | 195 |

Each stylesheet and full HTML binding is compressed independently, using Brotli quality 5/window 22 and gzip level 9. Other bindings form a sorted JSON-literal synthetic JavaScript stream; this measures the affected selector/token literals and excludes unrelated application JavaScript. Every per-stream row is derived from the frozen raw report's artifact prefixes, and CSS + HTML + affected JS sums exactly to the combined total for each codec and mode. The totals include the fixture markup and selector/token bindings, so adding classes to HTML cannot disappear from the score. They do not include the neutral browser harness or compiler manifests. An application integration must also measure its actual emitted bundle boundaries.

## Before and after

Two static owners share both declarations:

```css
.left{color:red;height:100px}.right{color:red;height:100px}
```

```html
<main><div data-probe="left" class="left">left</div><div data-probe="right" class="right">right</div></main>
```

The compiled result uses one shared bundle, keeping the HTML to one token per owner:

```css
.a{color:red;height:100px}
```

```html
<main><div data-probe="left" class="a">left</div><div data-probe="right" class="a">right</div></main>
```

The independent oracle finds the same **95 Brotli / 127 gzip / 127 raw bytes** across the two artifacts. It evaluates 246 leaf candidates within a declared finite domain of 12 candidate rectangles, at most four symbols, first-K names, and rule/declaration orders. The single-property fixture evaluates seven candidates and reaches **82 / 114 / 114 bytes**. This proves the minimum within those finite domains; the whole-application search is bounded and makes no global-optimum claim.

## What the browser checks

The harness compares authored source to guarded baseline, then baseline to compact. It checks all enumerated computed properties, explicitly observed aliases, pseudo-element styles, geometry, selector matches, focus/hover state, and byte-identical viewport PNGs. Small fixtures inspect every probe; the 512-node timing fixture samples approximately 32 representative probes and retains its full viewport screenshot. Each state also has an intentionally wrong CSS variant that must change both computed observations and the screenshot. Capability gates and authored expected values prevent unsupported declarations from producing a vacuous pass.

Coverage includes partial and noncontiguous bundles; intervening overrides; shorthand, fallback and custom-property behavior; logical/physical scroll insets under RTL and vertical writing modes; animation-range and mask-position aliases; layers, conditions, nesting, pseudo selectors and `:has()`; runtime class tokens; reserved and foreign tokens; escaped selectors; raw class-attribute whitespace; `@scope` and `@supports selector(...)`; incomplete inventory; and `noscript` with page scripting enabled and disabled.

An HTML binding rewrites actual class attributes only. Class observers inside raw `script` or `style` text must also appear as selector/token bindings and be rewritten by the consumer adapter, or their classes must be reserved. `complete_usage` remains the caller's assertion that it supplied all relevant usage. Classes inside opaque `noscript` fallback markup are automatically reserved.

The native run used Apple M4 Max, 16 logical CPUs, macOS, Node 24.12.0, Chrome 154.0.8037.93, and native ANGLE Metal GPU. Desktop is 960×720; mobile is 390×844; DPR is 1 and CPU shaping is 4×. It uses local Arial and no remote resources. These synthetic viewport checks are not physical Android certification.

Readiness waits for native fonts and six rendered frames with unchanged ResizeObserver dimensions. For scripting-disabled `noscript`, the harness leaves page callbacks disabled and checks six stable native layout samples after screenshot presentations. The common document reset fixes native tap-highlight and transient overlay-scrollbar styling for every mode; no screenshot pixels or computed properties are excluded. Source, baseline and candidate produced no unexpected requests or page exceptions.

The stock control matched the authored rendering in this particular 89-state run. It is an observational control, not the source of correctness, because separate repeated-process evidence demonstrates a rare upstream rule-loss defect.

## Counterexamples retained with the evidence

The evidence archive preserves failing observations from earlier compiler versions as well as the final passing run:

| Case | Observed failure | Required safeguard |
| --- | --- | --- |
| Logical scroll inset hoisting | A later logical 10px value became 20px after crossing a physical declaration; screenshots alone were unchanged. | Treat logical/physical aliases as cascade conflicts and inspect computed values. |
| Raw class boundary whitespace | Trimming a trailing or leading space introduced a previously unmatched `[class$=...]` or `[class^=...]` selector match. | Pin observed class lists and preserve the original string. |
| `noscript` inventory | With scripting disabled, an untouched fallback owner lost its CSS identity and a foreign class collided with a generated token. | Inventory opaque fallback markup, pin its owners and reserve its foreign names while retaining the raw source. |
| Upstream stale style index | In 512 independent unguarded builds, five dropped a visible final `.tail{color:blue}` rule. | Place temporary style-rule boundaries before upstream minification, remove them before printing, and stress deterministic output. |

The upstream reproducer is:

```css
.left{color:red;height:100px}.right{color:red}.right{height:100px}.tail{color:blue}
```

The 507 correct outputs retain `.tail{color:#00f}`; the five incorrect outputs omit it. These historical outputs use the prior unguarded compiler, whose SHA is recorded alongside the raw evidence. The final native fixture requires the authored tail to remain blue.

## Reproduce

Build with the pinned Rust toolchain, then run the evaluator and independent oracle:

```sh
cargo build --features cli
cargo build --example stock-control
node --test evaluation/*.test.mjs
node scripts/evaluate.mjs --bin=target/debug/lightningcss-compact --stock-bin=target/debug/examples/stock-control --output=/tmp/compact-functional --timing-pairs=0
node scripts/check-oracle.mjs --bin=/tmp/compact-functional/compiler.snapshot --output=/tmp/compact-oracle
```

On an otherwise quiet host, collect eight paired AB/BA timing samples against the same frozen executable:

```sh
node scripts/evaluate.mjs --bin=/tmp/compact-functional/compiler.snapshot --fixture=timing-many-nodes --output=/tmp/compact-timings --timing-pairs=8
node scripts/save-evidence.mjs --native=/tmp/compact-functional --oracle=/tmp/compact-oracle --timings=/tmp/compact-timings --output=evaluation/results --archive=/tmp/synthetic-evidence-2026-10-04.tar.gz
```

The evaluator freezes the binaries and records their SHA256, Chrome and GPU version, original observations, PNG hashes, readiness evidence, and full timing traces. Style, layout and task durations come from native performance metrics; Paint durations come from actual renderer-main `Paint` trace spans. The evidence writer rejects mismatched host, compiler, browser or GPU runs, omits derived compiler executables, and writes an archive manifest with the SHA256 of every retained evidence payload.

The committed `summary.json` is the compact machine-readable report. `streamTotals` records the corpus breakdown, and every fixture has `streamSizes` for each mode. Add `--summary-only=true` to the evidence-writer command to recompute that summary from the existing indexed reports while leaving the archive untouched; it verifies the original report hashes and every per-stream sum. `evidence-manifest.json` indexes the reports, traces and counterexample summaries; `evidence.sha256` identifies the separately distributed archive. Its internal `evidence-manifest.json` records every raw payload. Full observations, PNGs, compiler inputs/outputs, counterexamples and traces are kept in that archive rather than the Cargo package.

## Paired native timings

Eight fresh-document AB/BA pairs on the quiet host rendered the 512-node fixture against the same final compiler. Each of the 16 preserved traces contains two actual renderer-main Paint spans. The table reports milliseconds and the median of within-pair compact-minus-baseline deltas; that median need not equal the difference between the two independent medians.

| Measurement | Guarded median | Compact median | Paired delta median | Paired delta range |
| --- | ---: | ---: | ---: | ---: |
| Recalculate style | 2.236 | 2.260 | −0.083 | −0.454 to +1.191 |
| Layout | 2.439 | 2.054 | −0.156 | −1.109 to +0.226 |
| Main-thread tasks | 25.771 | 25.437 | +0.079 | −8.578 to +3.575 |
| Actual Paint spans | 0.630 | 0.541 | −0.030 | −0.583 to +0.818 |

Results are mixed, with positive and negative samples in every measurement. They do not establish a rendering-speed improvement. Paint is the union of native `Paint` trace intervals, not task duration or raster/GPU presentation time; the raw traces retain the exact events for inspection.

The evaluated shipping CLI SHA256 is `e64266db741b28eb6d71b6fca7041be2e5abae0b647dad9e0ae225b448d1227f`. Its source, guarded baseline and compact output passed the same native equality checks in both the functional and timing captures.

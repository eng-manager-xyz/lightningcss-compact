# Synthetic evaluation

The compiler preserves the authored rendering in all **36 fixtures and 99 native Chrome states**, and the harness detects all **99 deliberately incorrect variants**. Across this synthetic corpus, compact output reduces the independently compressed CSS, HTML and binding streams by **3.52% Brotli bytes against stock Lightning CSS alpha.72**, or **3.54% against the guarded baseline**. These are corpus results, not a forecast for another application.

The stock control is a separate executable that calls the upstream parser, default minifier and printer without the compact plugin. The guarded baseline uses the same upstream version with temporary rule boundaries to prevent a demonstrated upstream nondeterminism defect. Naming adds identity shortening; compact also searches safe declaration and bundle sharing. Across this corpus, the guarded baseline is one Brotli byte larger than stock: CSS adds 17 bytes while selector-binding printing removes 16 bytes from the affected JavaScript stream. The breakdown below exposes each contribution.

Combined CSS, HTML and affected JavaScript binding-stream totals:

| Output | Raw bytes | Brotli bytes | Gzip bytes |
| --- | ---: | ---: | ---: |
| Stock alpha.72 | 53,429 | 6,898 | 9,514 |
| Guarded baseline | 53,967 | 6,899 | 9,525 |
| Naming | 51,534 | 6,746 | 9,315 |
| Compact | 51,055 | 6,655 | 9,219 |

The same measurements separated by stream:

| Stream | Output | Raw bytes | Brotli bytes | Gzip bytes |
| --- | --- | ---: | ---: | ---: |
| CSS | Stock alpha.72 | 3,712 | 2,467 | 3,374 |
| CSS | Guarded baseline | 4,284 | 2,484 | 3,408 |
| CSS | Naming | 3,975 | 2,397 | 3,222 |
| CSS | Compact | 3,495 | 2,327 | 3,152 |
| HTML | Stock alpha.72 | 49,310 | 4,105 | 5,741 |
| HTML | Guarded baseline | 49,310 | 4,105 | 5,741 |
| HTML | Naming | 47,219 | 4,072 | 5,747 |
| HTML | Compact | 47,220 | 4,051 | 5,721 |
| Affected JS bindings | Stock alpha.72 | 407 | 326 | 399 |
| Affected JS bindings | Guarded baseline | 373 | 310 | 376 |
| Affected JS bindings | Naming | 340 | 277 | 346 |
| Affected JS bindings | Compact | 340 | 277 | 346 |

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

Coverage includes partial and noncontiguous bundles; intervening overrides; shorthand, fallback and custom-property behavior; logical/physical scroll insets under RTL and vertical writing modes; animation-range and mask-position aliases; layers, conditions, nesting, pseudo selectors and `:has()`; runtime class tokens; reserved and foreign tokens; escaped selectors; uppercase and namespaced class-attribute observers; immutable nested nth-selector observers; generated class text through direct, custom-property and escaped `attr(class)`; raw class-attribute whitespace; `@scope` and `@supports selector(...)`; incomplete inventory; and `noscript` with page scripting enabled and disabled.

An HTML binding rewrites actual class attributes only. Class observers inside raw `script` or `style` text must also appear as selector/token bindings and be rewritten by the consumer adapter, or their classes must be reserved. `complete_usage` remains the caller's assertion that it supplied all relevant usage. Classes inside opaque `noscript` fallback markup are automatically reserved. CSS declarations can observe the class string too: `attr(class)` and any namespaced, dynamic or otherwise uncertain `attr()` name pin the whole class namespace and preserve class-list whitespace. Proven literal reads of unrelated attributes remain eligible for naming. The broader conservative grammar guard also has Rust regression coverage; this Chrome corpus exercises the supported direct, custom-property and escaped forms.

The native run used Apple M4 Max, 16 logical CPUs, macOS, Node 24.12.0, Chrome 154.0.8037.93, and native ANGLE Metal GPU. Desktop is 960×720; mobile is 390×844; DPR is 1 and CPU shaping is 4×. It uses local Arial and no remote resources. These synthetic viewport checks are not physical Android certification.

Readiness waits for native fonts and six rendered frames with unchanged ResizeObserver dimensions. For scripting-disabled `noscript`, the harness leaves page callbacks disabled and checks six stable native layout samples after screenshot presentations. The common document reset fixes native tap-highlight and transient overlay-scrollbar styling for every mode; no screenshot pixels or computed properties are excluded. Source, baseline and candidate produced no unexpected requests or page exceptions.

The stock control matched the authored rendering in this particular 99-state run. It is an observational control, not the source of correctness, because separate repeated-process evidence demonstrates a rare upstream rule-loss defect.

## Counterexamples retained with the evidence

The evidence archive preserves failing observations from earlier compiler versions as well as the final passing run:

| Case | Observed failure | Required safeguard |
| --- | --- | --- |
| Logical scroll inset hoisting | A later logical 10px value became 20px after crossing a physical declaration; screenshots alone were unchanged. | Treat logical/physical aliases as cascade conflicts and inspect computed values. |
| Raw class boundary whitespace | Trimming a trailing or leading space introduced a previously unmatched `[class$=...]` or `[class^=...]` selector match. | Pin observed class lists and preserve the original string. |
| `noscript` inventory | With scripting disabled, an untouched fallback owner lost its CSS identity and a foreign class collided with a generated token. | Inventory opaque fallback markup, pin its owners and reserve its foreign names while retaining the raw source. |
| Uppercase class attributes | Renaming removed `[CLASS~=owner]` matches, borders and outlines. | Treat HTML class attribute names case-insensitively and retain namespace-qualified observations. |
| Immutable nth-selector attributes | A nested `:nth-child(... of [class~=owner])` observer lost its owner matches and border styles. | Recursively inventory class observations in selector subtrees that the serializer cannot rewrite. |
| Generated class text | Direct, custom-property and escaped `attr(class)` changed visible authored class strings to `"a"`. All three old-compiler captures fail both computed-style and PNG comparisons. | Inspect declaration tokens and functions; preserve identities and exact class-list strings wherever CSS can read them. |
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
node scripts/save-evidence.mjs --native=/tmp/compact-functional --oracle=/tmp/compact-oracle --timings=/tmp/compact-timings --output=evaluation/results --archive=/tmp/synthetic-evidence-observer-fix-2026-10-04.tar.gz
```

The evaluator freezes the binaries and records their SHA256, Chrome and GPU version, original observations, PNG hashes, readiness evidence, and full timing traces. Style, layout and task durations come from native performance metrics; Paint durations come from actual renderer-main `Paint` trace spans. The evidence writer rejects mismatched host, compiler, browser or GPU runs, omits derived compiler executables, and writes an archive manifest with the SHA256 of every retained evidence payload.

The committed `summary.json` is the compact machine-readable report. `streamTotals` records the corpus breakdown, and every fixture has `streamSizes` for each mode. Add `--summary-only=true` to the evidence-writer command to recompute that summary from the existing indexed reports while leaving the archive untouched; it verifies the original report hashes and every per-stream sum. `evidence-manifest.json` indexes the reports, traces and counterexample summaries; `evidence.sha256` identifies the separately distributed archive. Its internal `evidence-manifest.json` records every raw payload. Full observations, PNGs, compiler inputs/outputs, counterexamples and traces are kept in that archive rather than the Cargo package. The current archive is `lightningcss-compact-synthetic-evidence-observer-fix-2026-10-04.tar.gz` (5,046,508 bytes), SHA256 `94118cd5cf903ad55789de92d8f258ccd58eecce7b64b3a2613294fe5f629de0`, with 4,349 verified payloads. The public index includes each of the five old observer failure reports. The previous 31-fixture archive remains unchanged as historical evidence: `lightningcss-compact-synthetic-evidence-2026-10-04.tar.gz`, SHA256 `8767ab8d673cedcac639221128c0486f7ca763c6d5a9fa6ea135420470d55f31`.

## Paired native timings

Eight fresh-document AB/BA pairs on the quiet host rendered the 512-node fixture against the same final compiler. Each of the 16 preserved traces contains two actual renderer-main Paint spans. The table reports milliseconds and the median of within-pair compact-minus-baseline deltas; that median need not equal the difference between the two independent medians.

| Measurement | Guarded median | Compact median | Paired delta median | Paired delta range |
| --- | ---: | ---: | ---: | ---: |
| Recalculate style | 2.422 | 2.124 | −0.246 | −0.703 to 0.000 |
| Layout | 2.082 | 2.211 | +0.093 | −0.536 to +1.144 |
| Main-thread tasks | 25.918 | 25.958 | +0.078 | −9.602 to +2.483 |
| Actual Paint spans | 0.572 | 0.539 | −0.169 | −0.837 to +0.592 |

Results are mixed: style samples favor compact or tie, while layout, task and Paint samples include both signs. They do not establish a rendering-speed improvement. Paint is the union of native `Paint` trace intervals, not task duration or raster/GPU presentation time; the raw traces retain the exact events for inspection.

The evaluated shipping CLI SHA256 is `3a19092eab25d7b7cef8169ec249ae0a47efafb79e24af3b0b2864b79b09656b`. Its source, guarded baseline and compact output passed the same native equality checks in both the functional and timing captures.

# Algorithm and public contract

The optimizer plans a whole project's class mapping before changing any sheet.
It uses Lightning CSS `1.0.0-alpha.72` to parse, transform, minify, print, and
generate source maps. Authored input remains plain CSS. Framework-specific source
parsing and binding emission belong to consumer adapters.

## Inputs

`ProjectInput` accepts these fields. Missing fields use their empty or false
default; unknown JSON fields are errors.

| Field | Meaning |
| --- | --- |
| `stylesheets` | `{ id, source }` entries, with nonempty unique IDs and inline CSS sources |
| `bindings` | `{ id, kind, value }` entries, with nonempty unique IDs |
| `complete_usage` | Caller assertion that every class use which could change is accounted for |
| `managed_classes` | Names this project owns and may transform |
| `dynamic_classes` | Stateful names which must keep one removable identity |
| `reserved_classes` | Names whose spelling and membership must remain unchanged |
| `reserved_prefixes` | Protected namespaces; empty prefixes are rejected |
| `load_groups` | Ordered stylesheet-ID lists describing delivery combinations |

The library accepts logical stylesheet IDs. The CLI additionally requires safe,
portable relative `.css` paths for emitted CSS. It rejects traversal, reserved
device names, invalid path characters, trailing dots/spaces, and identities
which collide under case folding or Unicode normalization. It checks existing
output aliases, including hard links, before writing so they cannot replace the
project input. Loading groups must name existing inputs. They record delivery order;
the compiler does not concatenate sheets or move declarations between sheets.

Binding kinds serialize as `classes`, `selector`, `token`, and `html`.
`Classes` splits static class lists on HTML's ASCII whitespace. `Selector`
parses actual selector syntax and requires referenced identity markers to remain
available. `Token` accepts exactly one nonempty class token and automatically
treats it as dynamic. `Html` parses real class attributes with an HTML rewriter;
text, scripts, and code examples are not arbitrary replacement targets.

A complete inventory is a semantic assertion, not something the core can infer
from CSS. An adapter must include unstyled hooks and third-party observations,
reserve unsupported dynamic references, and extract class bindings from its
language's syntax. Missing usage can change which elements a renamed selector
matches. With `complete_usage: false`, names and class membership stay intact;
safe CSS-only selector grouping and ordinary minification remain available.

## Inventory and naming

The inventory visits selector AST components, including supported nested
functional selectors. Classes in complex selectors, scope contexts, and selector
bindings retain identities. Upstream stores `@supports selector()` as raw text;
the inventory parses valid selector text to discover and reserve its names while
leaving that condition unchanged. Where an upstream selector structure cannot
be rewritten safely, its names are reserved. Attribute selectors which
observe whole or partial class strings constrain both existing identities and
newly generated names. Reserved or dynamic owners never acquire an inseparable
expanded atom list.

Renamable identities and atoms start with a frequency allocation based on CSS
selector occurrences and bound usages. Equal frequencies use authored-name
order. Available names follow `a` through `z`, `A` through `Z`, then `aa`, `ab`,
and so on. Allocation excludes authored names, reserved namespaces, and names
which would newly match protected class-string observations. If collisions or
reservations exhaust the deterministic one-million-name search range, allocation
returns an inventory error instead of searching indefinitely.

A static owner can map to an identity plus shared atoms. Its identity can be
elided only when no remaining declaration selector or observed use requires it.
Class binding output has deterministic token order and deduplication when it
changes; unchanged class lists retain their original bytes. Class attribute
order never substitutes for CSS cascade order.

## Conservative factoring

Only flat style rules with one simple class selector participate as declaration
owners. Other selectors may still be renamed, but their declarations are not
treated as atomization owners. Candidates stay inside the same native rule list
and stylesheet, preserving conditional and layer context.

The collector considers single declarations, declaration pairs with repeated
support, complete groups with identical support, and complete owner bundles.
It also considers a bounded number of larger intersections. Each extracted
bundle must have the same declaration order in every contributing owner.

To move repeated declarations to their first occurrence, the collector proves
that the interval contains no conflicting writes. Property footprints use the
parser's shorthand-to-longhand relationships, augmented by conservative overlap
families for margin, padding, scroll-margin, scroll-padding, border, inset, size,
font, mask, animation, transition, and background. These families also cover
reset-only relationships and parsed aliases absent from upstream's shorthand
metadata, such as animation-range and mask-position. Unknown declarations,
custom properties, and `all` are barriers. Residual writes at both endpoints count, and intervening
nonflat rules or other rule boundaries partition the interval. This preserves
fallback/reset ordering even when moving identical text would look harmless.

An eligible interval can use a grouped selector, or a new shared class when
every owner is managed, static, and unreserved. Residual declarations remain at
their original positions. Generated rules keep one-class specificity, normal
versus important declarations, and their surrounding conditions. The final
Lightning CSS minification is also part of every measured representation.

## Minifier compatibility

Lightning CSS alpha.72 retains deduplication keys that refer to mutable rule
indices. Adjacent merging can change a key's rule or pop its index; a later
rule can reuse that index. Randomized hash-table probing can then change
deduplication or delete the later rule. Repeated independent builds exposed
different output and compression costs from identical input.

Every package mode inserts temporary unknown-at-rule boundaries between
adjacent style rules through the visitor API before upstream minification, and
removes them before printing or source mapping. This prevents adjacent key
mutation and index reuse. Declaration minification, ordinary printing, and
stable nonadjacent deduplication remain in Lightning CSS; this package's
deterministic factoring supplies shared rules. Original unknown at-rules are
preserved. The guard uses only default minifier targets; custom target options
are not part of this release's API.

The `baseline` mode is therefore a **guarded Lightning CSS control**, rather
than a claim of byte equality with an unguarded upstream build. Evaluation
also records ordinary upstream output separately, and the website compares
its frozen ordinary release with an upgrade-only control. Guard effects must
not be credited to class renaming or factoring. `apply_plan` installs the
already minified plan; adapters print it without minifying a second time.

## Search bounds and objective

`Options` defaults to `compact`. The other modes are `baseline` (guarded
Lightning CSS without class transformation) and `naming` (renaming without this
optimizer's declaration extraction).

| Option | Default | Maximum accepted | Scope |
| --- | --- | --- | --- |
| `search_limit` | 32 | 4096 | Additional larger intersections per collected rule list |
| `name_swap_limit` | 8 | 128 | Initial allocation-ranked identifiers considered for pair swaps |
| `max_evaluations` | 2048 | 65536 | Measured structural candidates in the general search |
| `max_rounds` | 32 | 256 | General structural-search rounds |

Zero bounds are valid. Mandatory singleton, pair, complete-support, and owner
bundle collection is not capped by `search_limit`. Name-swap measurements and
the separately bounded exact domain are not charged against
`max_evaluations`; consequently `candidates_evaluated` can exceed that option.
These are search limits, not hard limits on parsing time or input memory.

General search evaluates real emitted CSS and bindings for each candidate,
accepts the best strict improvement, and repeats until stable or bounded. Its
comparison orders combined `(brotli, gzip, raw)` bytes, with the additional
requirement that a step does not increase gzip. A subsequent bounded pair-swap
pass measures naming changes against the same objective.

The final output rolls back to baseline if either its combined Brotli or gzip
total exceeds baseline. This protects the supplied inventory's encoded cost,
not an application's unprovided markup or script bundles. It does not establish
a globally optimal CSS representation or a rendering-speed improvement.

## Exact finite domain

A separate exhaustive path applies only when all these conditions hold:

- Complete usage, one stylesheet, and exactly two flat single-class owner rules.
- Two distinct managed owners with no reserved, dynamic, or observed identities.
- The same ordered one or two unique declarations in both rules, without
  `!important`, nesting, or conflicting property footprints.
- At most 8192 bytes across input binding values.

Within this domain the compiler enumerates all states reachable through its
safe grouping and atom contractions, including intermediate representations
which increase size. It enumerates remaining rule permutations, independent
declaration permutations, and all allocations of the first K available short
names to K live identities/atoms. It keeps the unchanged baseline as an option
and requires final gzip to remain no greater than that baseline. Its fixed HTML
serialization is part of the representation family; arbitrary token order,
arbitrary names, unrelated CSS rewrites, and larger programs are outside it.

The independent JavaScript oracle in `evaluation/oracle.mjs` has its own finite
rectangle-cover formulation. Its published fixtures use two owners and one or
two independent declarations, up to six declaration occurrences, four naming
symbols, and twelve candidate rectangles. It enumerates exact nonoverlapping
covers, rule/declaration permutations, first-K alphabet allocations, and its
declared anchor-first/sorted-atom HTML order. It prunes invalid covers and excess
symbols, never by heuristic compressed-size estimates. Equality with those
fixture minima is evidence for those declared cases, not a general optimality
proof or a claim that every oracle representation matches the compiler's
serialization family.

## Encoding boundaries

All core measurements use Brotli quality 5 with window 22 and gzip level 9.
Raw means UTF-8 byte length. Each stylesheet is its own compressed stream. Each
HTML binding is its own compressed stream, so a caller should use whole rendered
documents when evaluating document transfer cost.

Non-HTML bindings, ordered by binding ID, are JSON-encoded string literals
followed by `;`, combined into one synthetic JavaScript/template stream. The
report's `javascript` field measures that stream. It does not measure complete
JavaScript sources, surrounding template syntax, minifier interactions, or
consumer bundle boundaries. The `total` field adds those CSS, HTML, and synthetic
literal costs. Maps, manifests, reports, and `result.json` are build artifacts
outside that transfer objective.

`baseline`, `naming`, and `optimized` report these exact boundaries. `optimized`
describes returned bytes, including any rollback; `naming` records the initial
naming representation. Native evaluation separately compares computed styles,
geometry, and viewport pixels and intentionally tests broken cascade/binding
controls. Its synthetic timing measurements remain informational. Integrations
must measure their actual served HTML/CSS/JavaScript streams and application
behavior independently.

## Output and adapter contract

`CompiledProject` contains generated `stylesheets`, bound `bindings`,
`source_maps`, `manifest`, and `report`. The manifest currently has
`schema_version: 1`, the package `compiler_version`, and a SHA-256 `generation`
of emitted CSS, bindings, class expansions, and load groups. It records retained
`identities`, expanded `classes`, input `source_hashes`, output `output_hashes`,
`load_groups`, and stable `load-group-N` chunk metadata. Chunk metadata does not
create bundled files or permit reordered delivery.

`prepare_project` returns an immutable reusable plan. `apply_plan` checks the
incoming stylesheet against the canonical prepared input and applies the planned
native Lightning CSS rules. It returns the whole project's report. Consumers
should parse with the same filename and parser behavior used for preparation;
later transforms or printer options may change bytes and invalidate the original
cost/hash claim. `compile_project` returns the prepared output directly.

Source maps retain original stylesheet content and locations through final
minification. Extracted declarations inherit their contributing source location;
the compiler does not invent a unique original source for every shared use. Map
generation must emit byte-identical CSS, and compilation fails if it does not.
CLI maps sit beside generated CSS without automatic browser map-link comments.

Adapters emit literal bindings during their own compilation. They must neither
rewrite prose/code fences nor translate DOM already copied from a compiled
document a second time. No runtime class lookup or stylesheet registry is needed.
The Rust/CLI release does not include an npm visitor or framework adapter.

## Deployment generations

The library produces generation data; applications enforce it. CSS, scripts,
and markup must be published as one compatible generation. Soft navigation must
check a destination's generation before loading incompatible styles or promoting
its DOM. Asset hashes must identify their exact bytes; an obsolete hash must not
serve current bytes under an old URL.

Initial stale HTML is also a deployment boundary: recovery JavaScript cannot
run if that document's uncached script URL already disappeared. Retain old assets
for the HTML cache horizon or provide an independent bootstrap/document-refresh
mechanism. Test online recovery with real cache behavior, bound recovery loops,
keep offline content readable, and verify retained documents with unchanged
generations. A manifest marker alone does not prove cache-safe deployment.

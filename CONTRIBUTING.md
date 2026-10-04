# Contributing

Keep the optimizer framework-neutral. Template integrations extract usage and
consume the manifest; they do not belong in the CSS optimizer's core. Do not
introduce browser stylesheet injection or runtime class lookup.

## Checks

Run from the repository root:

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo test --locked --no-default-features
cargo package --locked --list
cargo package --locked
cargo run --locked --features cli -- build --project examples/project.json --out-dir target/example-output
cargo run --locked --example bundle
python3 scripts/third-party-licenses.py --check
```

CI repeats format, Clippy, and test checks on Rust 1.88.0 and current stable on
Linux, macOS, and Windows. It installs the CLI from the unpacked Cargo package,
so a successful checkout build cannot hide missing packaged files. Keep Cargo.lock
committed for reproducible CLI and release checks.

Regenerate `licenses/third-party/` with `python3 scripts/third-party-licenses.py`
when dependencies or CLI features change. The generator copies actual upstream
license/notice files for the native release target union and records missing
texts explicitly. Keep the tree's original bytes; `.gitattributes` prevents
platform newline conversion. Review missing notice entries before distribution.
When a Cargo package omits its notice files, record supplemental upstream files
under `licenses/upstream/` with exact VCS revisions, source URLs, and content
hashes. The generator verifies those records without fetching mutable upstream
files. Do not invent missing copyright notices or silently replace them with an
SPDX label.

For changes to transformation behavior, include a case that would fail if the
cascade changed, not just a snapshot of the generated implementation. Cover
conflicting class combinations, shorthand/longhand order, layers, importance,
conditional rules, loading groups, dynamic tokens, and preserved third-party
identifiers. Compare library and CLI output. Test deterministic output with
shuffled discovery, repeated builds, and platform path conventions.

The independent evaluator requires Node.js 24 and a native Chrome installation.
Its local fixtures do not need an npm installation or remote content:

```sh
cargo build --locked --features cli
node --test evaluation/*.test.mjs
node scripts/check-oracle.mjs --bin=target/debug/lightningcss-compact --output=target/oracle-evidence
node scripts/evaluate.mjs --bin=target/debug/lightningcss-compact --output=target/chrome-evidence --timing-pairs=2
```

Set `CHROME_BIN` when Chrome is not discovered, or pass the evaluator's `--chrome`
option. On Windows use the `.exe` binary suffix. The native evaluator freezes a
copy of the executable, preserves raw evidence, and checks deliberate cascade
and binding mutants. Do not weaken its assertions to admit a transformation.
The exhaustive oracle covers only the finite fixtures and representation family
declared in [ALGORITHM.md](docs/ALGORITHM.md); it is not a proof of general CSS
optimality. CI uploads the independent evidence even when evaluation fails.

Measure actual compressed CSS plus consumer markup and script costs when making
size claims. A smaller uncompressed stylesheet or shorter selector alone does not
prove lower transfer cost or faster rendering. Preserve source locations in
diagnostics and explain why candidate declarations remain residual.

Keep encoder boundaries reproducible. The core's `javascript` report is a
synthetic bound-literal stream, not a whole application JavaScript bundle.
Native fixture timing is informational and does not establish production or
physical-phone performance. Application adapters must evaluate their actual
rendered assets, cache generations, and navigation behavior separately.

## Compatibility

Public manifest versions are distinct from Lightning CSS AST versions. Changes
to public inputs or manifests need an explicit compatibility decision and a
changelog entry. Keep the upstream AST adapter pinned until its compatibility
tests justify an upgrade. Test the advertised minimum Rust version rather than
inferring it from the edition.

Keep explicit inventory failures for invalid inputs and exhausted allocation
bounds. Increasing supported bounds changes resource behavior and needs evidence.
Adapters must preserve unsupported class observations and keep authored source
examples out of binding extraction. Do not turn unknown references into global
string replacements.

Do not copy application content, private build conventions, or upstream licensed
source into generic fixtures without preserving the applicable license. Original
contributions use MIT OR Apache-2.0, matching this project.

## Releases

1. Update the package version and CHANGELOG, and complete CI on the release
   commit. Review `cargo package --list` and the verified package contents.
2. Check that the tag `vX.Y.Z` matches Cargo.toml. Tagging starts a workflow that
   repeats CI and builds native CLI archives with license files, dependency
   notices, and checksums.
3. The workflow creates a draft GitHub release only after every archive and smoke
   check passes. Inspect it before publication. This workflow does not publish to
   crates.io.
4. Publish the Cargo package separately when authorized, then verify installation
   from the registry with `cargo install lightningcss-compact --features cli
   --locked` on supported hosts.

Linux archives use the native GNU target. They are not musl/static executables.
Release notes should list tested operating systems and architectures; do not claim
untested platform support from a cross-compilation result alone.

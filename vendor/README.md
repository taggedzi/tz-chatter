# Temporary glib safety backport

`glib-0.18.5/` is the complete published MIT-licensed crate. Its version is unchanged. The application workspace overrides crates.io glib through `[patch.crates-io]` in `src-tauri/Cargo.toml`; the test-only `glib-variant-regression` workspace member shares that override and lockfile. The application remains the default workspace member.

## Provenance and exact change

- Advisory: [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html), alias [GHSA-wrw7-89jp-8q8g](https://github.com/advisories/GHSA-wrw7-89jp-8q8g).
- Official fix: [gtk-rs-core PR 1343](https://github.com/gtk-rs/gtk-rs-core/pull/1343).
- Source archive: [glib 0.18.5](https://static.crates.io/crates/glib/glib-0.18.5.crate).
- Archive SHA-256: `233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5` (verified against the original Cargo.lock checksum before extraction).
- Upstream commit: `42b9caf98e03ded086362d9653ca58fe94dc8658` (also in `.cargo_vcs_info.json`).
- License and notices: `glib-0.18.5/LICENSE` and `glib-0.18.5/COPYRIGHT`.

The only changes to the published files are in `src/variant_iter.rs`: `let p` becomes `let mut p`, and the C output argument `&p` becomes `&mut p`. `glib-0.18.5.upstream.json` records each original archive file's SHA-256. `tests/glibBackport.test.mjs` reverses precisely those two edits in memory and verifies every upstream file. A clean extraction of the archive can independently reproduce the checksum manifest.

## Verification

On Linux with Rust, a C toolchain, pkg-config, and GLib development headers (`libglib2.0-dev` on Ubuntu):

```sh
node --test tests/glibBackport.test.mjs
cargo test --locked --release --manifest-path src-tauri/Cargo.toml -p glib-variant-regression
cargo tree --locked --manifest-path src-tauri/Cargo.toml --target x86_64-unknown-linux-gnu -i glib
```

The regression tests call all five affected iterator operations (`next`, `next_back`, `nth`, `nth_back`, `last`), forward/reverse iteration, UTF-8 and empty strings, and exhaustion. Optimizations are required because the original invalid shared reference can cause the compiler to discard the C output write. CI and release validation run the optimized test. The test contains Linux-only code; a Windows run with zero tests is not evidence of this fix.

## Scanner handling

This is a source backport, not an upstream version upgrade. Version-based scanners can still flag 0.18.5, while scanners that skip local path dependencies can stop reporting it. Neither behavior proves the source safe: use the integrity and optimized runtime tests. Keep the true version and do not add a blanket advisory ignore. The GitHub alert is not dismissed by this change. The dependency should still be included with its MIT license in release SBOMs; do not treat a local dependency as application-owned code.

## Removing the backport

1. Confirm the upstream release includes PR 1343 (or an equivalent fix). If the fixed release fits the GTK stack's `^0.18` constraints, updating is sufficient. If it belongs to another series such as 0.20+, first update the Tauri/GTK/WebKit dependencies to versions compatible with that series. Merely adding a newer glib direct dependency does not replace the old one.
2. Remove the glib entry from `[patch.crates-io]` (and the empty section), then run `cargo update --manifest-path src-tauri/Cargo.toml -p glib`. Cargo will continue using the local crate until the override is removed. Adjust the test package's glib requirement if the dependency family changes.
3. Remove this vendored crate, its checksum manifest, and the backport-specific Node test. Keep the optimized regression and workflow steps to verify the upstream replacement.
4. Inspect the Linux reverse dependency tree and Cargo.lock: there must be no affected glib version left. Run the optimized regression, application tests/build, formatting/Clippy, and dependency audit. Verify GitHub's alert state after the change is merged.

Do not remove the patch simply because a scanner becomes quiet or because an incompatible fixed glib version exists.

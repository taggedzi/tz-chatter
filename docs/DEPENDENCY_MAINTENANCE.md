# Dependency maintenance

Keep dependency updates reviewable and preserve any security backports until an upstream replacement is verified.

## Before updating

1. Check `vendor/README.md` for active source patches and their removal conditions.
2. Inspect the dependency tree and platform scope. A dependency may be reachable only on Linux, macOS, or Windows through a platform-specific Tauri backend.
3. Read the upstream changelog and advisory before changing a version or removing a patch.

## After updating

Run the applicable checks from the repository root:

```sh
npm ci
npm run test:unit
npm run lint
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo audit --file src-tauri/Cargo.lock
```

For Linux dependency or GTK/Tauri changes, also run the optimized safety regression:

```sh
cargo test --locked --release --manifest-path src-tauri/Cargo.toml -p glib-variant-regression
cargo tree --locked --manifest-path src-tauri/Cargo.toml --target x86_64-unknown-linux-gnu -i glib
```

Review the lockfile diff for unrelated upgrades, run `git diff --check`, and record the versions, advisories, platform checks, and any unavailable environments in `docs/STATUS.md` and `docs/HANDOFF.md`.

## Active glib backport

The Linux GTK graph currently uses a vendored `glib 0.18.5` with the two-line fix from gtk-rs-core PR 1343 for RUSTSEC-2024-0429. Cargo selects it through `[patch.crates-io]`, and the regression harness points directly at the vendored crate so Dependabot does not create a duplicate version alert; do not replace it with an unrelated direct glib dependency or add a blanket audit ignore. Version-based scanners may continue to report the alert because the crate version remains 0.18.5.

When a compatible upstream release contains the fix, remove the Cargo patch, update the lockfile, keep the optimized regression, and remove the vendored crate only after the resolved tree contains no affected glib. If the fixed release is from an incompatible series, update the Tauri/GTK/WebKit stack first. Follow the complete procedure in `vendor/README.md`.

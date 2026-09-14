# Verifying tz-chatter downloads

Official downloads come only from the [tz-chatter GitHub Releases page](https://github.com/taggedzi/tz-chatter/releases). Windows packages are intentionally not Authenticode-signed, so Windows may display a SmartScreen warning. Verification establishes which source repository, commit, and workflow produced a file; it does not guarantee that software is free of defects.

Replace the example version and filename below with the release you downloaded.

## Recommended: verify build provenance

Install the [GitHub CLI](https://cli.github.com/), then run:

```powershell
gh attestation verify .\tz-chatter-v0.2.0-windows-x64-setup.exe --repo taggedzi/tz-chatter
```

```bash
gh attestation verify ./tz-chatter-v0.2.0-linux-x86_64.AppImage \
  --repo taggedzi/tz-chatter
```

A successful result must identify `taggedzi/tz-chatter` as the source repository.

## Verify the immutable GitHub release

```text
gh release verify v0.2.0 --repo taggedzi/tz-chatter
```

You can also compare a local download directly with the immutable release asset:

```text
gh release verify-asset v0.2.0 PATH_TO_DOWNLOADED_FILE --repo taggedzi/tz-chatter
```

## Verify SHA-256 checksums

Download `SHA256SUMS` beside the package. On Linux:

```bash
sha256sum --check SHA256SUMS --ignore-missing
```

On Windows, calculate the hash and compare it with the matching line in `SHA256SUMS`:

```powershell
Get-FileHash .\tz-chatter-v0.2.0-windows-x64-setup.exe -Algorithm SHA256
Get-Content .\SHA256SUMS
```

The hexadecimal values must match exactly; letter case does not matter.

## Inspect the signed SBOM attestation

The release includes a readable SPDX 2.3 JSON SBOM. The same SBOM is also attached cryptographically to each executable package. Verify and print that attestation with:

```text
gh attestation verify PATH_TO_DOWNLOADED_FILE --repo taggedzi/tz-chatter --predicate-type https://spdx.dev/Document/v2.3
```

Do not run a package when verification fails, names a different repository, or was downloaded from an unofficial mirror.

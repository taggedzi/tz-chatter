# Releasing tz-chatter

This is the complete maintainer checklist for the project's verifiable unsigned release process. It intentionally does not use Authenticode, paid certificates, signing programs, package repositories, or an automatic updater.

## One-time setup

- Keep GitHub two-factor authentication enabled for the maintainer account.
- In the repository, open **Settings → General → Releases** and enable **Immutable releases**. The release workflow refuses to publish while this is disabled.
- Leave the repository public so standard GitHub-hosted runners and artifact attestations remain available without charge.

## Release checklist

- [ ] Choose a SemVer version such as `0.2.0-beta.1` or `0.2.0`.
- [ ] From a clean `main` checkout, run `./scripts/prepare-release.ps1 VERSION`.
- [ ] Review, commit, and push the five version-manifest changes.
- [ ] Wait for CI and CodeQL to pass on `main`.
- [ ] Open **Actions → Release → Run workflow** on GitHub.
- [ ] Enter the same version without a leading `v`. Choose **dry-run** to test safely, **prerelease** for a preview, or **stable** only after the release gate is complete.
- [ ] After the workflow succeeds, confirm that the GitHub release displays **Immutable**.
- [ ] Download and verify one package using [VERIFYING.md](VERIFYING.md).

The preparation command never creates a tag or publishes a release. It is safe to run again if a previous version update was interrupted and the only uncommitted files are the five version manifests.

## What GitHub publishes

- `tz-chatter-vVERSION-windows-x64-setup.exe`
- `tz-chatter-vVERSION-linux-x86_64.AppImage`
- `tz-chatter-vVERSION.sbom.spdx.json`
- `SHA256SUMS`

The manual workflow validates the version and complete source gates, generates an SPDX 2.3 JSON SBOM, builds on clean Windows and Ubuntu 22.04 GitHub-hosted runners, creates GitHub/Sigstore build-provenance and SBOM attestations for both packages, and assembles a complete release candidate. The default **dry-run** mode stops there and retains the candidate as a seven-day workflow artifact. A publishing mode creates a draft and publishes it only after every expected asset is present.

## If something stops

- **Preparation reports unrelated changes:** finish, commit, or stash those changes, then rerun the same command.
- **CI or Release is red:** open the failed job and fix the named error. Nothing is published unless the final publish job succeeds.
- **A draft release exists:** do not start a release with a different version. Inspect the draft and failed workflow first; ask for help if the safe recovery is unclear.
- **The release workflow says immutable releases are disabled:** perform the one-time setting above, then rerun the workflow.
- **The tag or release already exists:** do not move, delete, or reuse it. Prepare a new patch version instead.

Release artifacts are deliberately unsigned at the Windows Authenticode layer. Every release and download page must keep the SmartScreen warning and verification instructions visible.

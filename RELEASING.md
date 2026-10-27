# Releasing tz-chatter

This is the complete maintainer checklist for the project's verifiable unsigned release process. It intentionally does not use Authenticode, paid certificates, signing programs, package repositories, or an automatic updater.

## One-time setup

- Keep GitHub two-factor authentication enabled for the maintainer account.
- In the repository, open **Settings → General → Releases** and enable **Immutable releases**. The release workflow refuses to publish while this is disabled.
- Leave the repository public so standard GitHub-hosted runners and artifact attestations remain available without charge.
- Configure the Actions secret `RELEASE_ADMIN_READ_TOKEN` with a fine-grained personal access token restricted to this repository and **Administration: read** (plus required Metadata read). Do not grant Administration write or use this token for release writes. The built-in `GITHUB_TOKEN` cannot request Administration read. A GitHub App installation token with that same read permission is also supported, but must be refreshed before expiry.
- Run the Release workflow with mode **check-immutability** and the committed version to verify the actual hosted administrative read without building, creating drafts/tags, publishing, or changing settings. This mode may run on a review branch; run only trusted code because it receives the read credential. It succeeds only for JSON boolean `enabled: true`. A disabled setting is a successful administrative read but a failed release gate. The full **dry-run** build remains separate and needs no administrative secret.

The [immutable-release endpoint](https://docs.github.com/en/rest/repos/repos#check-if-immutable-releases-are-enabled-for-a-repository) requires Administration read. For publishing modes, the workflow checks it alongside source validation, then checks again immediately before draft creation. Only an explicit boolean `enabled: true` permits progress. Disabled, missing/nonboolean fields, malformed JSON, request/authentication errors, timeouts, and missing credentials all block; no repository setting is changed automatically. Release writes continue to use the short-lived built-in token with Contents write.

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
- **The administrative read fails:** check the secret's expiry, selected repository, and Administration read permission, then run **check-immutability** again. A 401/403/404 or network error is an unverified setting, not evidence that immutability is disabled. Do not bypass the check.
- **The tag or release already exists:** do not move, delete, or reuse it. Prepare a new patch version instead.

Release artifacts are deliberately unsigned at the Windows Authenticode layer. Every release and download page must keep the SmartScreen warning and verification instructions visible.

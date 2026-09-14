# tz-chatter

[![CI](https://github.com/taggedzi/tz-chatter/actions/workflows/ci.yml/badge.svg)](https://github.com/taggedzi/tz-chatter/actions/workflows/ci.yml)
[![CodeQL](https://github.com/taggedzi/tz-chatter/actions/workflows/codeql.yml/badge.svg)](https://github.com/taggedzi/tz-chatter/actions/workflows/codeql.yml)

A desktop application for chatting with AI characters powered by models running on the user's computer. Each character has an editable definition and a portable Markdown memory vault. The application retrieves relevant memories, records significant conversational events, and can initiate conversation when the user enables it.

## Project navigation

- [AGENTS.md](AGENTS.md): instructions for any agent continuing development.
- [Project specification](docs/PROJECT.md): goals, architecture, data ownership, product behavior, and unscheduled possible later features.
- [Self-maintaining hybrid memory spec](docs/specs/2026-09-12-self-maintaining-hybrid-memory-design.md): accepted design (ADR-009). Implementation: T24/T25.
- [Self-maintaining hybrid memory plan](docs/specs/2026-09-12-self-maintaining-hybrid-memory-plan.md): executor detail for T24/T25.
- [Implementation plan](docs/PLAN.md): ordered tasks, dependencies, and acceptance criteria.
- [Live status](docs/STATUS.md): authoritative task states, current work, verification, and next action.
- [Decision log](docs/DECISIONS.md): accepted decisions and their rationale.
- [Handoff log](docs/HANDOFF.md): chronological record of completed work and continuation notes.
- [Completion audit](docs/AUDIT.md): release gaps discovered by the independent completion review and their remediation evidence.
- [Release checklist](RELEASING.md): the short maintainer process for versioned Windows and Linux packages.
- [Download verification](VERIFYING.md): provenance, immutable-release, checksum, and SBOM verification commands.

Start with the live status to see what is actually implemented. Design descriptions are not implementation claims.

## Development setup

Tauri 2 uses Rust for the desktop core and React/TypeScript with Vite for the UI. The current scaffold uses the npm package manager.

Prerequisites for Windows:

- Node.js and npm
- Rust with the MSVC target
- Microsoft C++ Build Tools with Desktop development with C++
- Microsoft Edge WebView2 Runtime (included on most supported Windows versions)

Prerequisites for the recorded Ubuntu 24.04 GNOME/Wayland run (T33):

- Node.js 20+ and npm (Ubuntu 24.04's Node 18 is too old for this Vite 8 frontend)
- Rust stable (`rustup`)
- Tauri 2 system libraries: `libwebkit2gtk-4.1-dev`, `libjavascriptcoregtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`, `libayatana-appindicator3-dev`, `patchelf`, `libxdo-dev`, plus `build-essential`, `curl`, `wget`, `file`, and `libssl-dev`

On a VirtualBox shared folder (`vboxsf`), do not run `npm install` or Cargo in the share: that filesystem cannot create symlinks, and Windows `node_modules`/`target` trees are the wrong platform. Copy the sources to native ext4 and set `CARGO_TARGET_DIR` there. SQLite vault indexes should also live on native disk.

Windows is the verified desktop target. Linux was tested on Ubuntu 24.04.5 LTS with GNOME on Wayland (T33, VirtualBox guest); that is not a general Linux support claim. macOS is intended to compile and uses Cmd in the keymap; that is not a support claim, and there is no Mac download or macOS prerequisite list.

Install JavaScript dependencies and verify the frontend:

```powershell
npm install
npm run lint
npm run typecheck
npm run build
```

Verify the Rust core:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
```

Run the desktop shell. This starts Vite, compiles the Rust core, and opens a Tauri window:

```powershell
npm run tauri dev
```

Create a local Windows build when the environment has the required packaging tools:

```powershell
npm run tauri build
```

On the recorded Ubuntu 24.04 host the same npm scripts apply from a native-disk checkout (not the VirtualBox share):

```bash
npm install
npm run lint
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run tauri dev
npx tauri build --bundles deb,appimage
```

## Continuous integration

GitHub Actions runs the frontend unit tests, ESLint, TypeScript checks, frontend build, Rust tests, rustfmt, and Clippy on Ubuntu 24.04 for pushes and pull requests targeting `main`. The same CI workflow audits npm and Cargo dependencies for known vulnerabilities. A separate CodeQL workflow scans the TypeScript and Rust sources on pushes, pull requests, manual runs, and a weekly schedule.

See [the CI workflow](.github/workflows/ci.yml) and [the CodeQL workflow](.github/workflows/codeql.yml). The badges at the top of this README report the latest default-branch result for each workflow.

## Releases and trust

The manual [Release workflow](.github/workflows/release.yml) is the only supported way to publish binary packages. It validates a synchronized version and the complete source gates, then builds a Windows x64 NSIS installer and Linux x86_64 AppImage on clean GitHub-hosted runners. Each release includes SHA-256 checksums, an SPDX 2.3 JSON SBOM, and GitHub/Sigstore build-provenance and SBOM attestations. Safe **dry-run** is the default; publishing assembles a draft first and requires GitHub immutable releases to be enabled.

Maintainers use the one-page [release checklist](RELEASING.md). Users can follow [VERIFYING.md](VERIFYING.md) to verify a download against `taggedzi/tz-chatter`, its immutable GitHub release, and its checksums.

These packages are intentionally not Authenticode-signed. Windows may display a SmartScreen warning; a self-signed or paid certificate and enrollment in a signing service are outside the selected release scope. Provenance establishes where and how an artifact was built, not that the application is free of defects. Do not publish while `docs/STATUS.md` says the release gate is on hold.

The current build includes provider contracts and local connections, Markdown-backed vault persistence, lexical and hybrid retrieval, memory review/extraction, bounded initiative delivery, native tray/notification hooks, and validated portable character packs. Check `docs/STATUS.md` before relying on any feature claim; T18 release validation remains in progress.

Portable character packs are created from Settings → Vault. Export writes a new directory containing canonical Markdown and durable operational state; import validates the manifest, rejects traversal and unrelated-vault overwrite, rebuilds the search index, and keeps a backup when explicitly replacing the same character.

## First run

1. Start a local provider. For Ollama, run `ollama serve` and make sure the configured model is available, for example `ollama run llama3.2`. For LM Studio, start the local server from the Developer tab (default `http://127.0.0.1:1234/v1`) and pick **LM Studio** in Settings → Provider.
2. Launch tz-chatter. Chat is the default view. Open **Characters** to create a vault or add an existing folder; use **Browse…** instead of typing a path when convenient. The editor separates Identity, Model, You, and Scenes so each kind of data has its own save action. Optional portraits live at `assets/portrait.png` — choose a PNG in the Identity section or drop that file into the character folder. Each character can override the chat model, temperature, and max tokens; empty fields use Settings → Provider. Those overrides are stored in `.tz-chatter/generation.json`, not in `character.md`, and are omitted from portable packs. The bundled sample is `examples/characters/lyra` — it includes Lyra's definition, a user persona, a cafe scene, six starter memories, and a welcome chat. The chat header picks a session scene from those files.
3. The sidebar lists saved conversations from `chats/` by title (auto-filled from the first user message, or rename one). Search chats, or archive a session without deleting its Markdown. Open the welcome chat to see the sample history, or **New** to start a distinct session. Ask about Mina or Markdown notes to exercise retrieval.
4. Send a message with Enter (Shift+Enter for a new line). The composer emoji button opens an in-app picker; click a glyph to insert it at the caret. Turns are written to Markdown under `chats/`. Edit the last user message, regenerate a complete reply, or continue an interrupted one. Retry remains for failed or empty replies. Previous complete takes stay in the transcript as superseded so extraction does not double-commit. Retrieved memory sources appear in the Sources panel when available. Completed replies queue extraction; supported facts are saved as Markdown memories without an Accept click. Open Memories to fix mistakes, review guesses, or resolve Conflicts. See **Keyboard shortcuts** below.
5. Use **Memories** to browse or correct Markdown memories. The optional Review Queue and Conflicts sections follow the main browser and stay compact when empty. Exclude a stale conflict side to drop it from retrieval without deleting it. Open **Settings** for provider connection (the inherit-default chat model), the application prompt (sent before character identity), initiative, and portable-pack export/import. Vault and pack fields include native folder pickers; export and restore parent selection fills a new child folder that can be renamed before the action. Credentials stay in application configuration rather than character packs.
6. To enable spontaneous initiative, open Settings → Initiative after loading a conversation, enable it, and save. This starts the local 30-second scheduler for the active character/session and resumes it after restart; loading another vault stops the old scheduler. Disable the setting and save to stop it. Initiative is disabled by default and respects inactivity, quiet hours, cooldowns, unanswered messages, and daily caps.

## Keyboard shortcuts

Mouse controls stay. On macOS the same chords use Cmd; that is a portable binding, not a macOS support claim. Session rename still uses Escape to cancel.

- **Ctrl+N** — new conversation on the current character, then focus the composer
- **Ctrl+K** — typeahead character switcher; Enter loads that vault and stays on Chat
- **Ctrl+L** — go to Chat and focus the composer
- **Ctrl+.** — stop generation
- **Ctrl+I** — toggle Sources when the last turn retrieved memories
- **Escape** — close the switcher, then Settings, then the emoji picker, then the new-local form, then stop generation, then Sources, then focus the composer. Session rename still owns Escape.

The Windows release is currently validated with a live Ollama smoke test. Ollama and OpenAI-compatible protocol paths have deterministic coverage, but a live llama.cpp/LM Studio endpoint has not been available for this release. MSI and NSIS bundles are produced under `src-tauri/target/release/bundle/`.

Linux was tested on Ubuntu 24.04.5 LTS GNOME/Wayland (T33). That run produced `tz-chatter_0.1.0_amd64.deb` (5,294,836 bytes) and `tz-chatter_0.1.0_amd64.AppImage` (81,373,688 bytes). It is not a claim that every Linux distro or desktop is supported. macOS builds are not claimed. The Linux chat-turn evidence used a local Ollama-shaped stub because Ollama was not installed on that guest.

## Troubleshooting and release notes

- `Rust core is unavailable`: close and relaunch the desktop executable, or run `npm run tauri dev` from a development checkout.
- Opening a vault fails: select the vault directory itself, not its parent; it must contain a valid `character.md`. Use Characters → Add, or portable-pack import, to restore a validated vault into a new directory.
- Provider failures: verify the endpoint and model name in Settings → Provider, and confirm the local server is running. tz-chatter does not download models or silently fall back to a cloud provider.
- Notifications are opt-in in Settings and can also be restricted by Windows notification permissions. Initiative is disabled by default.
- Windows release packages are intentionally unsigned and may show the normal SmartScreen warning. Download only from the official GitHub Releases page and follow `VERIFYING.md` before running them.

## Source control

The project uses Git with `main` as its initial branch and is hosted at [taggedzi/tz-chatter](https://github.com/taggedzi/tz-chatter). Use `git status` to inspect changes and `git log --oneline` to inspect history.

Commit source files, documentation, and application lockfiles. Build/dependency folders, local environment files, downloaded models, and root-level `characters/` runtime vaults are ignored. Keep intentional sample characters in `examples/` and synthetic test vaults in `tests/fixtures/` so they can be versioned. Keep private data outside tracked sample directories.

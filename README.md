# tz-chatter

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

Start with the live status to see what is actually implemented. Design descriptions are not implementation claims.

## Development setup

Tauri 2 uses Rust for the desktop core and React/TypeScript with Vite for the UI. The current scaffold uses the npm package manager.

Prerequisites for Windows:

- Node.js and npm
- Rust with the MSVC target
- Microsoft C++ Build Tools with Desktop development with C++
- Microsoft Edge WebView2 Runtime (included on most supported Windows versions)

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

The current build includes provider contracts and local connections, Markdown-backed vault persistence, lexical and hybrid retrieval, memory review/extraction, bounded initiative delivery, native tray/notification hooks, and validated portable character packs. Check `docs/STATUS.md` before relying on any feature claim; T18 release validation remains in progress.

Portable character packs are created from Settings → Vault. Export writes a new directory containing canonical Markdown and durable operational state; import validates the manifest, rejects traversal and unrelated-vault overwrite, rebuilds the search index, and keeps a backup when explicitly replacing the same character.

## First run

1. Start a local provider. For Ollama, run `ollama serve` and make sure the configured model is available, for example `ollama run llama3.2`. For LM Studio, start the local server from the Developer tab (default `http://127.0.0.1:1234/v1`) and pick **LM Studio** in Settings → Provider.
2. Launch tz-chatter. Chat is the default view. Open **Characters** to create a vault or add an existing folder. New, Add, and Save all use identity, user persona, and locals (`character.md`, `persona.md`, `scene.md`, `locals/`). Optional portraits live at `assets/portrait.png` — choose a PNG in the editor or drop that file into the character folder. Each character can override the chat model, temperature, and max tokens; empty fields use Settings → Provider. Those overrides are stored in `.tz-chatter/generation.json`, not in `character.md`, and are omitted from portable packs. The bundled sample is `examples/characters/lyra` — it includes Lyra's definition, a user persona, a cafe local, six starter memories, and a welcome chat. The chat header picks a session scene from those local files.
3. The sidebar lists saved conversations from `chats/` by title (auto-filled from the first user message, or rename one). Search chats, or archive a session without deleting its Markdown. Open the welcome chat to see the sample history, or **New** to start a distinct session. Ask about Mina or Markdown notes to exercise retrieval.
4. Send a message with Enter (Shift+Enter for a new line). Turns are written to Markdown under `chats/`. Edit the last user message, regenerate a complete reply, or continue an interrupted one. Retry remains for failed or empty replies. Previous complete takes stay in the transcript as superseded so extraction does not double-commit. Retrieved memory sources appear in the Sources panel when available. Completed replies queue extraction; supported facts are saved as Markdown memories without an Accept click. Open Memories to fix mistakes, review guesses, or resolve Conflicts.
5. Use **Memories** to review or correct Markdown memories, including **Conflicts** for committed files that contradict or supersede each other. Exclude the stale file to drop it from retrieval without deleting it. Open **Settings** for provider connection (the inherit-default chat model), the application prompt (sent before character identity), initiative, and portable-pack export/import. Credentials stay in application configuration rather than character packs.
6. To enable spontaneous initiative, open Settings → Initiative after loading a conversation, enable it, and save. This starts the local 30-second scheduler for the active character/session and resumes it after restart; loading another vault stops the old scheduler. Disable the setting and save to stop it. Initiative is disabled by default and respects inactivity, quiet hours, cooldowns, unanswered messages, and daily caps.

The Windows release is currently validated with a live Ollama smoke test. Ollama and OpenAI-compatible protocol paths have deterministic coverage, but a live llama.cpp/LM Studio endpoint has not been available for this release. MSI and NSIS bundles are produced under `src-tauri/target/release/bundle/`.

## Troubleshooting and release notes

- `Rust core is unavailable`: close and relaunch the desktop executable, or run `npm run tauri dev` from a development checkout.
- Opening a vault fails: select the vault directory itself, not its parent; it must contain a valid `character.md`. Use Characters → Add, or portable-pack import, to restore a validated vault into a new directory.
- Provider failures: verify the endpoint and model name in Settings → Provider, and confirm the local server is running. tz-chatter does not download models or silently fall back to a cloud provider.
- Notifications are opt-in in Settings and can also be restricted by Windows notification permissions. Initiative is disabled by default.
- The local MSI and NSIS artifacts are unsigned and are not externally published. Windows may show the normal SmartScreen warning for an unsigned local build.

## Source control

The project uses Git with `main` as its initial branch. Use `git status` to inspect changes and `git log --oneline` to inspect history. No remote is configured yet.

Commit source files, documentation, and application lockfiles. Build/dependency folders, local environment files, downloaded models, and root-level `characters/` runtime vaults are ignored. Keep intentional sample characters in `examples/` and synthetic test vaults in `tests/fixtures/` so they can be versioned. Keep private data outside tracked sample directories.

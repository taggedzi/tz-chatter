# Implementation plan

This document defines work and acceptance criteria. `STATUS.md` is the sole live task-state tracker. Execute dependencies first; task order within a milestone can change when dependencies allow it.

## D00 — Project planning and continuity

Deliver the project specification, implementation plan, authoritative status file, decision log, handoff log, and agent entry point. A new agent must be able to identify the goal, implemented state, next task, and verification expectations without the original conversation.

## Milestone 1 — Desktop and provider connection

### T01 — Scaffold the desktop application

Depends on: D00.

Create Tauri 2, React/TypeScript, and Rust application structure, including configuration, lockfiles, lint/type/build checks, and an initial desktop shell. Inspect available toolchains and current official documentation before selecting dependency versions. Record exact setup/run commands in README. Initialize a local Git repository if one is still absent; a remote and publishing are not prerequisites.

Acceptance: frontend and Rust checks pass; launch the desktop shell on Windows and verify the window opens. Record a blocked runtime verification honestly if the environment cannot support it. Establish a repeatable validation command set.

### T02 — Define provider contracts and configuration

Depends on: T01.

Implement typed health, discovery, chat streaming, cancellation, embedding, and capability boundaries. Persist endpoint/model preferences separately from character data. Establish validated commands/events between Rust and the UI.

Acceptance: contract tests cover unavailable endpoints, invalid configuration, unsupported capabilities, and cancellation. Settings survive restart. Optional credentials are excluded from logs and character exports.

### T03 — Define character and transcript storage

Depends on: T01.

Formalize versioned character, memory, transcript, and operational-state formats. Add one original sample character. Implement load/create/validate behavior, stable IDs, path confinement, and safe persistence primitives.

Acceptance: a fixture vault round-trips without losing message roles, multiline Markdown, IDs, or metadata; invalid schemas and paths produce actionable errors; author-controlled identity is separated from writable memories. Interrupted writes preserve the previous valid file.

### T04 — Connect Ollama and compatible providers

Depends on: T02.

Implement Ollama chat/discovery and OpenAI-compatible streaming for llama.cpp and LM Studio, with capability-specific behavior. Handle partial stream frames, errors, completion, and cancellation.

Acceptance: recorded or mocked transport tests cover each provider protocol and fragmented streams; verify discovery and one real streamed reply against an available local engine. Record each named provider's live verification separately; contract tests alone are not a live compatibility claim.

## Milestone 2 — Usable character chat

### T05 — Build the conversation lifecycle

Depends on: T03, T04.

Connect character selection, composer, streamed replies, stop/retry behavior, and transcript persistence. Snapshot character/session IDs per request and order writes consistently.

Acceptance: restart resumes persisted history; failed/cancelled replies are correctly marked; switching characters during generation cannot place replies in the wrong vault; retries do not duplicate user messages.

### T06 — Build budgeted character prompts

Depends on: T05.

Assemble rules, character definition, scene context, recent turns, and the current message. Add token budgeting with reserved output capacity and a conservative fallback estimator.

Acceptance: tests cover ordering, oversized history, an oversized current message/character, and context-limit recovery. The actual serialized request uses the selected character and does not repeat the current message accidentally.

## Milestone 3 — Manual memory and inspectable retrieval

### T07 — Build the memory vault and lexical index

Depends on: T03, T05.

Implement memory CRUD, external-change detection, Markdown chunking, SQLite FTS5 indexing, and full rebuild. Excluded/deleted records must not remain searchable.

Acceptance: create, edit, rename, and delete a memory externally and observe the corresponding index changes; rebuild from Markdown produces equivalent search results; concurrent edits are detected; character isolation is tested.

### T08 — Retrieve memories into chat

Depends on: T06, T07.

Add lexical retrieval, source labels, deduplication, salience/recency controls, pinned-memory budgets, and archival context in prompts.

Acceptance: a deterministic fixture conversation retrieves a relevant earlier fact; unrelated and excluded memories are omitted; retrieval stays inside its context budget and selected character.

### T09 — Add memory browsing and context inspection

Depends on: T08.

Build memory browsing/editing and a per-turn inspector showing selected sources, retrieval reasons, and prompt-budget allocation. Include source navigation and explicit lock/exclude/delete actions.

Acceptance: a user can trace a reply's supplied memory to its source and correct it; the next turn sees the changed data. Show estimated token counts as estimates when appropriate.

Milestone outcome: the first complete memory-backed chat loop works with manually authored Markdown memories and no embedding model.

## Milestone 4 — Automatic memory formation

### T10 — Extract validated memory proposals

Depends on: T08.

Create a durable extraction queue and structured-output parser with source verification, retry bounds, and user-chat priority. Handle engines/models that cannot reliably enforce a JSON schema.

Acceptance: malformed output, invented source IDs, unavailable models, and interrupted turns cannot corrupt the vault; retries remain bounded and resume after restart; extracted assistant speculation is not automatically promoted to user fact.

### T11 — Reconcile and commit memories

Depends on: T07, T10.

Deduplicate proposals, retain contradiction/supersession links, respect locked files, suppress reconstruction of deleted memories, and commit accepted candidates atomically before indexing.

Acceptance: duplicate job delivery produces one memory; old transcript reprocessing respects deletion suppression; newer user edits survive background work; a failed index update can be repaired from committed Markdown.

### T12 — Add memory review and correction

Depends on: T09, T11.

Expose pending proposals, supporting excerpts, accept/edit/reject controls, and automatic-save preferences. Route uncertain proposals to review and define a conservative default for potentially sensitive inferred information.

Acceptance: review decisions persist across restart; rejection does not repeatedly resurface unchanged; user corrections affect future retrieval; transcript deletion and memory deletion have distinct, understandable behavior.

## Milestone 5 — Semantic retrieval

### T13 — Add embeddings and versioned indexes

Depends on: T04, T07, T11.

Implement separate embedding-model selection, chunk fingerprints, embedding-space identity, SQLite vector storage, and rebuild jobs. Begin with direct similarity search suitable for measured vault sizes.

Acceptance: model/dimension/chunk changes invalidate incompatible data; no mixed embedding spaces are searched; interrupted rebuilds recover; lack of an embedding model leaves lexical chat functional.

### T14 — Implement and evaluate hybrid ranking

Depends on: T08, T13.

Combine lexical and semantic candidates with entity/link, salience, and recency signals; diversify results and expose reasons. Establish a small curated retrieval evaluation set.

Acceptance: evaluation includes paraphrases, exact names, stale/contradictory memories, distractors, and character isolation. Record recall and latency for lexical and hybrid retrieval on the same fixtures; document regressions and chosen tradeoffs before enabling defaults.

## Milestone 6 — Spontaneous engagement

### T15 — Implement persisted initiative eligibility

Depends on: T05, T11.

Add opt-in settings, quiet hours, inactivity/cooldown/frequency limits, ignored-message backoff, and persisted scheduling state in the desktop core.

Acceptance: tests use a controllable clock to cover quiet-hour boundaries, restart, sleep/resume, disabled settings, and frequency caps; no catch-up burst or initiative occurs while disabled.

### T16 — Generate and deliver initiative turns

Depends on: T12, T15.

Retrieve open topics, allow the model to choose silence, and deliver a labeled spontaneous turn. Add tray behavior and separately configurable notifications; background work yields to active chat.

Acceptance: new user input or character changes cancel stale work; ignored messages trigger backoff; generated questions do not fabricate facts or pollute memory; quitting stops scheduling. Verify the actual Windows tray and notification behavior.

## Milestone 7 — Portability and release readiness

### T17 — Export, import, and recover character data

Depends on: T12, T13, T16.

Implement validated portable character packs, backup/restore, schema migration tests, and index repair. Preserve operational state where appropriate without copying machine credentials or absolute paths.

Acceptance: export/import round-trip preserves identity, memories, transcripts, and durable review/deletion state; traversal paths are rejected; index corruption is recoverable; restore behavior is tested without overwriting unrelated vaults.

### T18 — Validate and package the Windows release

Depends on: T14, T17.

Run the complete user journey, accessibility/keyboard checks, failure/recovery scenarios, and provider compatibility checks. Measure representative storage/retrieval performance. Produce a local Windows package and clear install/use/troubleshooting documentation.

Acceptance: packaged application launches; persisted conversation and recalled memories survive restart; all three providers receive a documented live smoke test or the release explicitly narrows its support claim. Record tested hardware/models, known limitations, and packaging/signing status. Publishing externally is outside this task.

### T19 — Remediate completion-audit findings

Depends on: T17.

Resolve every finding in `docs/AUDIT.md`, including canonical-memory edit safety, vault/character validation, active-character UI state, integrated hybrid retrieval, real UI streaming, foreground-priority extraction recovery, natural-language retrieval, local-time initiative behavior, live initiative delivery, context inspection, tray behavior, strict validation, and source-control readiness.

Acceptance: every audit row is resolved with evidence; regression tests cover data integrity and character isolation; hybrid retrieval is reachable with visible lexical fallback; incremental stream events reach the UI; pending extraction drains on resume without delaying chat; initiative persists one labelled turn and updates the active UI; lint, typecheck, Rust tests, strict Clippy, packaging, and executable launch pass. Actual Windows tray/notification/keyboard interaction must be recorded separately.

### T20 — Consolidate the chat layout

Depends on: T19.

Replace the landing-page shell and scattered controls with a chat-program layout. The default surface is the conversation: a selectable session list, a full-height transcript, and a composer. Provider, initiative, and portability controls move into a dedicated Settings overlay. Memories remain a secondary view that uses the loaded character instead of duplicate vault fields.

Acceptance: Conversation is the default view and shows history without provider forms; Settings contains provider, initiative, and vault/portability sections; the sidebar lists persisted sessions and opening one loads that transcript; a new conversation starts a distinct session; frontend lint/typecheck/build pass; Rust tests cover session list/open/start.

### T21 — Character library and application prompt

Depends on: T20.

Add a Characters primary view that lists known vaults, creates new portable character folders, and edits author-controlled identity in `character.md`. Persist the library in application configuration. Add a Settings → Prompt section for the global application rules, stored outside character vaults, and prepend those rules to every chat and initiative request before character identity and memories. Empty saved rules omit that extra message.

Acceptance: a user can create a character, add an existing vault, edit identity fields, and switch the active character from the library; creating refuses to overwrite an existing `character.md`; missing vaults remain listed with an error; the application prompt is first in the serialized request when present and omitted when empty; extraction still cannot edit `character.md`; frontend lint/typecheck/build pass.

### T22 — Populate the provider chat model list from discovery

Depends on: T04, T21.

Settings → Provider should list chat-capable models reported by the connected provider instead of relying on a free-text field with hidden autocomplete. Run discovery when the panel loads and when connection details change; keep a manual refresh; preserve a typed custom name when discovery fails or the saved model is not in the provider list.

Acceptance: the chat model control is populated from `provider_discover`; the saved model remains selectable if it is absent from discovery; a custom name can still be entered; frontend lint/typecheck/build pass.

### T23 — First-class LM Studio provider settings

Depends on: T04, T22.

Add LM Studio as a named Settings → Provider kind with the same health, discovery, chat, and embedding flow as Ollama. Use the existing OpenAI-compatible HTTP transport and LM Studio’s default local server (`http://127.0.0.1:1234/v1`). Keep a generic OpenAI-compatible option for other servers such as llama.cpp.

Acceptance: selecting LM Studio applies a distinct id and default endpoint; health/discovery/chat use `/v1` OpenAI-compatible paths; previously saved `open_ai_compatible` settings still load; Rust provider tests and frontend lint/typecheck/build pass. A live LM Studio smoke test is recorded if an endpoint is available, otherwise the limitation stays explicit.

## Milestone 8 — Self-maintaining hybrid memory

Spec: `docs/specs/2026-09-12-self-maintaining-hybrid-memory-design.md`. Executor detail: `docs/specs/2026-09-12-self-maintaining-hybrid-memory-plan.md`. Chat is the only required user action; reviewing Memories is optional.

### T24 — Auto-write validated memories

Depends on: T12.

After a completed extraction job, classify each proposal’s origin against the transcript. Auto-accept and commit user-stated facts that quote a user turn, plus validated conversation events and character/world facts, as **new** Markdown files. Leave inferred guesses (including assistant-only “user_stated” labels) in the existing Memories inbox. Do not mutate existing memory bodies. Locked files, deletion suppression, and duplicates keep current reconciler behavior. Wire this into `process_extraction_queue` so the user does not click Accept.

Acceptance: tests prove auto-write of a user-stated candidate to a new file and FTS5 hit; inferred and assistant-only “user_stated” stay inbox-only with no file; duplicates do not write a second file; supersession creates a new file and leaves the old body unchanged; locked related memories and deleted/suppressed sources do not write; the extraction worker test after resume auto-commits the cafe fixture and leaves an inferred fixture in the queue. `cargo test --manifest-path src-tauri/Cargo.toml` passes.

### T25 — Hybrid retrieval by default

Depends on: T14, T24.

When the active provider has an embedding model and hybrid is not opted out, use hybrid retrieval. If the vector index is missing, stale, or mid-rebuild, do not embed on the chat path: use lexical, record a fallback reason, and enqueue a background embed job that takes the model gate and yields to chat. Skip memory IDs that are the target of a `supersedes` relationship so old files are not injected as current. Default the UI hybrid flag on (`localStorage !== "false"`).

Acceptance: tests prove superseded memories are not retrieved as current; hybrid mode is used when an embedding model is set and the index is ready; a not-ready index does not call embed during send and falls back to lexical with a rebuild reason; no embedding model still lexical-falls-back; the T14 evaluation fixture still holds. Frontend lint/typecheck pass. README states that chatting accumulates memories without an Accept step.

## Milestone 9 — Persona and reusable locals

### T26 — User persona and scene locals

Depends on: T06, T17, T21.

Add first-class portable Markdown for who the user is and where the scene is, outside `character.md`. Store `persona.md` and a `locals/` library of reusable scene documents in the character vault, with `scene.md` holding the character default local id. A session may follow that default, select another existing local, or clear the scene; it cannot point at a file that is not already in `locals/`. Resolve live at send time (edit `cafe.md`, the next turn reads the new text). Prompt order: application rules, character identity, user persona, current scene, memories, history, current message. Empty sections are omitted. Extraction still cannot write identity, persona, or locals. Packs include the new files.

Acceptance: tests cover persona/local round-trip; default vs session override vs none; missing local fails send; live-link after editing a local; extraction cannot write `persona.md`/`locals/`; pack export/import preserves persona, scene default, and locals. Frontend lint/typecheck/build pass. Characters can edit persona and locals and set the default; chat can pick a session local from existing files or create a local then select it.

## Milestone 10 — Character portraits

### T27 — Character portraits

Depends on: T17, T21.

Render `assets/portrait.png` in the Characters library list, the chat sidebar active-character card, and the character identity editor. Users can add or replace the file from the editor (PNG file picker or drag-drop) or by placing `assets/portrait.png` in the vault folder. Remove deletes that file. Missing portraits fall back to initials. Writes stay confined to the selected vault. Packs include the portrait when present. Open-folder / Obsidian remain unscheduled.

Acceptance: create scaffolds `assets/`; set/clear round-trip `assets/portrait.png`; reject non-PNG and files larger than 5 MB; missing portrait is not an error; a PNG dropped into `assets/portrait.png` is detected on the next library list; pack export/import preserves the file; frontend lint/typecheck/build pass.

## Milestone 11 — Session names, search, and archive

### T28 — Session names, search, and archive

Depends on: T20.

Give the conversation sidebar readable titles, transcript search, and archive without deleting Markdown. Store optional `title` and `archived` in transcript YAML; keep filenames as `{session_id}.md` in `chats/`. Auto-title from the first user turn on the next transcript save when `title` is empty; never overwrite a non-empty title. Rename writes `title` immediately. Archive sets `archived: true` in place. The default list hides archived sessions; a sidebar control shows them. Search is a case-insensitive substring over title and turn bodies for the loaded character and includes archived hits with a snippet. No new SQLite table; Markdown remains canonical. Packs already include `chats/*.md`.

Acceptance: files without `title`/`archived` still load; auto-title uses the first user turn and does not replace a rename; archive hides from the default list while `chats/{id}.md` remains and unarchive restores it; search matches title and body, stays character-isolated, and returns archived hits; frontend lint/typecheck/build pass.

## Milestone 12 — Edit, regenerate, and continue

### T29 — Edit last message, regenerate, and continue

Depends on: T05, T10, T24.

Extend the last exchange so users can rewrite the last user turn, request another assistant reply, and continue an interrupted reply. Retry remains for failed or empty replies. Add transcript status `superseded` (schema still 1). Regenerating a complete reply keeps the previous assistant text in Markdown as `superseded`, mints a new assistant id, and extracts only the replacement. Continue appends to the same Interrupted turn id and extracts once when that turn becomes Complete. Edit last user updates that turn in place, supersedes following assistant turns, then generates a new assistant id. Prompt history and the chat UI omit superseded turns. Extraction jobs whose assistant source is no longer Complete are abandoned without writing memories. Do not delete memories from superseded replies. Last exchange only: no editing older turns, no swipe carousel, no continue of Complete replies.

Acceptance: legacy transcripts without `superseded` still load; regenerate keeps the old assistant in the file as superseded and extracts only the new complete id; pending extraction of a superseded source does not commit; edit changes the last user turn and does not overwrite a sticky session title; continue appends to Interrupted with the same id and extracts once on Complete; retry of a failed empty reply does not drop earlier superseded siblings; frontend lint/typecheck/build pass.

## Milestone 13 — Per-character model and sampling

### T30 — Per-character model and sampling

Depends on: T02, T05, T21.

Remember optional chat model, temperature, and max tokens per character. Provider connection (kind, endpoint, token, embedding model) stays app-global; Settings → Provider chat model is the inherit default. Store overrides in `{vault}/.tz-chatter/generation.json`, never in `character.md`. Empty fields inherit the active provider. Chat and initiative use the resolved model and sampling. Extraction uses the character’s chat model name with conservative sampling (temperature 0, its own max tokens), not the character’s creative temperature or cap. Embeddings stay on the global embedding model. Packs omit `generation.json` unless a later opt-in export is added. Resolve in Rust at send/schedule time.

Acceptance: a character override changes the chat and initiative model/temperature/max tokens while another character without an override keeps the provider default; extraction for an overridden character uses that model name with conservative sampling; `character.md` is unchanged; export does not include generation settings; invalid temperature or max tokens are rejected; frontend lint/typecheck/build pass.

## Milestone 14 — Contradiction and supersession review

Spec: `docs/specs/2026-09-13-contradiction-supersession-review-design.md`.

### T31 — Contradiction and supersession review

Depends on: T11, T12.

Show committed `contradicts` and `supersedes` pairs on Memories. Read durable `memory_relationships` in `.tz-chatter/state.sqlite3`, join both Markdown records, and hide `related`, missing, or excluded sides. Resolve by excluding the chosen file (`review_status: excluded`); do not delete, mutate bodies, or write links into Markdown. Contradicts: the user picks which side to exclude. Supersedes: Exclude older targets `to_memory_id` only on this command. A pair leaves the list when either side is excluded or deleted. User-initiated exclude of a locked file is allowed. Clicking a side opens the existing editor.

Acceptance: contradicts pairs appear with both excerpts; `related` links do not; a pair disappears after exclude or delete of either side; missing sides are omitted; other vaults are isolated; exclude of one contradicts side leaves the other accepted, drops FTS hits, and keeps the file browsable; supersedes exclude-older rejects `from_id` on this command; locked `to` can be excluded; a stale pair errors and writes nothing; the list command does not write; frontend lint/typecheck/build pass.

## Milestone 15 — Keyboard-first chat

### T32 — Keyboard-first chat

Depends on: T20, T21.

Add a Windows-first chat keymap so the existing loop (new session, switch character, focus composer, stop generation, open sources) works without the mouse. Mouse controls stay. No new Rust commands or storage.

Bindings: Ctrl+N new session; Ctrl+K typeahead character switcher; Ctrl+L focus composer; Ctrl+. stop generation; Ctrl+I toggle Sources when memories were retrieved; Escape closes in order (switcher, Settings, stop generation, Sources, then focus composer). Cmd mirrors Ctrl on macOS without claiming macOS verification. The switcher filters the existing character library, loads the chosen vault through the current resume path, stays on Chat, and focuses the composer. It does not open the identity editor. Ignore IME composition and key-repeat on new session. Session rename keeps Escape to cancel.

Acceptance: the resolver covers the keymap and Escape stack; the switcher can pick a library character and resume that vault; composer focus works after new session, switch, and Ctrl+L; Cancel/Sources/New still work with the mouse; frontend lint/typecheck/build pass. Native chord click-through remains environment-limited.

## Milestone 16 — Linux verification

### T33 — Linux desktop verification

Depends on: T17.

Run the desktop app on a Linux host with a display. This is evidence, plus code only if that run finds a Linux-specific bug. Do not wait for T16/T18 Windows leftovers. WSL2/WSLg is not a general Linux claim; if that is the only host, record it as WSL.

On the Linux host: install Node, Rust, and Tauri 2 system libraries (WebKitGTK 4.1, AppIndicator, rsvg, OpenSSL, build tools). Then record:

1. Frontend lint/typecheck/build, `cargo test`, Clippy, and fmt.
2. `npm run tauri dev` — the window opens and responds.
3. Add or load a vault copied from Windows (sample Lyra or a Settings → Vault pack).
4. Send one turn; transcript and any memory write land on disk.
5. Restart; conversation and memories are still there.
6. Tray icon and, if enabled, one initiative notification — or name the desktop and the gap (GNOME without AppIndicator often has no tray).
7. `npm run tauri build` — keep the artifacts that actually appeared (AppImage and/or `.deb`). Do not require both.

Acceptance: Linux launch, vault round-trip, and one chat turn are evidenced in `STATUS.md` with distro/desktop. Packaging artifacts and tray/notification results are named. Failures are written down, not inferred from Windows. README may then say Linux was tested on that distro/desktop; it must not say “Linux is supported” in general. Add Linux prerequisites and run commands only after this run.

Out of scope: macOS, GitHub Actions, signing, publishing, live llama.cpp/LM Studio, and finishing T16 visual tray on Windows.

## Unscheduled feature backlog

Possible later features live in `docs/PROJECT.md`. Do not add further IDs until the user accepts a specific item into this plan with dependencies and acceptance criteria.

Remaining suggested cluster (not scheduled): memory inbox chrome, remember-this-from-a-turn, and open-vault-as-files (folder / Obsidian / reveal; portraits are T27; session names/search/archive are T28; edit/regenerate/continue is T29; per-character model/sampling is T30; contradiction/supersession review is T31; keyboard-first chat is T32; Linux verification is T33). Named generation presets and opt-in pack export of sampling remain unscheduled. macOS verification remains unscheduled until a Darwin host can launch the window (ADR-017).

## Milestone exit policy

A milestone is complete only when its tasks are `done`. Mark task criteria separately if an implementation is ready but a required runtime check is unavailable. Do not infer tested operating systems, models, or providers from shared code paths.

## D01 — Establish Git source control

Depends on: D00.

Initialize the local repository on `main`, configure ignores and text normalization, commit the planning baseline, and update the handoff. No remote or publishing is required.

Acceptance: verify the committed files, ignored runtime/build paths, trackable source/lockfiles/samples, and a clean working tree. This completes the Git initialization portion of T01 only; the desktop scaffold remains separate.

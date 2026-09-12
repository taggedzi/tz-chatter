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

## Unscheduled feature backlog

Possible later features live in `docs/PROJECT.md`. They have no task IDs and are not the next work except T24 and T25 above. Do not add further IDs until the user accepts a specific item into this plan with dependencies and acceptance criteria.

Remaining suggested cluster (not scheduled): memory inbox chrome, remember-this-from-a-turn, and open-vault-as-files.

## Milestone exit policy

A milestone is complete only when its tasks are `done`. Mark task criteria separately if an implementation is ready but a required runtime check is unavailable. Do not infer tested operating systems, models, or providers from shared code paths.

## D01 — Establish Git source control

Depends on: D00.

Initialize the local repository on `main`, configure ignores and text normalization, commit the planning baseline, and update the handoff. No remote or publishing is required.

Acceptance: verify the committed files, ignored runtime/build paths, trackable source/lockfiles/samples, and a clean working tree. This completes the Git initialization portion of T01 only; the desktop scaffold remains separate.

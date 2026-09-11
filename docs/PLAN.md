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

## Milestone exit policy

A milestone is complete only when its tasks are `done`. Mark task criteria separately if an implementation is ready but a required runtime check is unavailable. Do not infer tested operating systems, models, or providers from shared code paths.

## D01 — Establish Git source control

Depends on: D00.

Initialize the local repository on `main`, configure ignores and text normalization, commit the planning baseline, and update the handoff. No remote or publishing is required.

Acceptance: verify the committed files, ignored runtime/build paths, trackable source/lockfiles/samples, and a clean working tree. This completes the Git initialization portion of T01 only; the desktop scaffold remains separate.

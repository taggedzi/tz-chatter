# Handoff log

Append entries chronologically. Keep current status in STATUS.md; this file explains how the project reached that state. Include enough concrete detail that another agent can resume without conversation history.

## 2026-09-11 — D00: planning and continuity system

Scope: translated the user's approved character-chat architecture into a durable specification, dependency-ordered implementation plan, and file-based status/handoff workflow.

Files: `README.md`, `AGENTS.md`, `docs/PROJECT.md`, `docs/PLAN.md`, `docs/STATUS.md`, `docs/DECISIONS.md`, and `docs/HANDOFF.md`.

Decisions: ADR-001 through ADR-007 capture the approved design. Planning clarifies that disposable indexes and durable operational state have separate storage roles, canonical transcripts need a round-trip schema, and external local providers are the first integration mode.

Environment evidence: workspace was empty and not a Git repository. No application scaffold, dependencies, tests, or running local services were created or verified. No previous project conversations were consulted.

Verification: documentation checks and their outcomes are recorded in STATUS.md. No application checks apply to this documentation-only milestone.

Next action: T01. Inspect the actual toolchains and Tauri prerequisites, then scaffold and verify the desktop shell using the specification. T03 will formalize data formats; do not mistake the proposed vault layout for existing code.

## 2026-09-11 — D01: local Git source control

Outcome: initialized `main` and committed the nine project files in `c0743bf` (`chore: initialize project planning and source control`). Added `.gitignore`, `.gitattributes`, README source-control guidance, and a D01 plan/status entry. Application scaffolding remains unstarted.

Verification: nine representative runtime/build paths are ignored; seven source, lockfile, environment-example, and sample paths remain trackable. Staged whitespace checks passed; the tree was clean after the baseline commit; no remote is configured. This entry and the completed status are recorded in a subsequent documentation commit.

Environment recovery: sandbox ownership of `.git` triggered Git's ownership check and the sandbox helper failed intermittently. Attempts to transfer ownership and move the directory did not resolve directory ownership. Empty initialization metadata was preserved at `C:/Users/tagge/AppData/Local/Temp/tz-chatter-git-init-2b53cd5605854fd1963fa88d20dde334`; it contains no project history. Reinitialized metadata in place and added only this project's exact path to the user's Git `safe.directory` setting. Existing Git author identity was used unchanged. No project files or prior commits were deleted.

Next action: T01 — inspect toolchains and scaffold the desktop application. Use the existing repository; no further Git initialization is needed.

## 2026-09-11 — T01: desktop scaffold

Scope and outcome: created the Tauri 2 desktop shell with React/TypeScript/Vite frontend and Rust core. Replaced the generator demo with a local-first tz-chatter landing shell, added a typed `app_info` command, and documented repeatable setup/validation commands.

Files changed: `package.json`, `package-lock.json`, `index.html`, `eslint.config.js`, `src/`, `src-tauri/`, `README.md`, `.gitignore`, and the T01 status record. The official generator removed tracked continuity files during its forced scaffold; those exact files were restored from the baseline commit before work continued.

Decisions added/superseded: none. The bundle identifier is `com.tzchatter.desktop`; the scaffold uses Tauri 2, React 19, Vite 8, TypeScript 6, and the current Rust-compatible Tauri crates resolved in `Cargo.lock`.

Verification and actual results: `npm run lint`, `npm run typecheck`, `npm run build`, `cargo check --manifest-path src-tauri\\Cargo.toml`, `cargo fmt --manifest-path src-tauri\\Cargo.toml --all -- --check`, and `npx tauri build --no-bundle` all passed. `npm run tauri dev` compiled and opened a responsive native `tz-chatter` window, verified via process metadata (`Responding=True`).

Incomplete work / blockers: the Computer Use helper failed to initialize, so no screenshot-based visual QA was recorded. No provider integration, character vault, transcript persistence, or tests beyond scaffold validation are claimed.

Next concrete action: begin T02 provider contracts/configuration or T03 character/transcript storage; both now depend only on completed T01.

## 2026-09-11 — T02: provider contracts and configuration

Scope and outcome: added provider-neutral contracts for health, model discovery, streamed chat, cancellation, embeddings, capabilities, and structured failures. Added validated provider configuration and app-config persistence independent of character vaults. The UI has matching TypeScript types and typed invoke helpers.

Files changed: `src-tauri/src/providers.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `src/providers.ts`, `Cargo.lock`, and the T02 status record.

Decisions added/superseded: none. Ollama exposes a baseline embedding capability; OpenAI-compatible transports expose chat/cancellation but embeddings remain negotiated instead of assumed across llama.cpp and LM Studio.

Verification and actual results: `cargo test --manifest-path src-tauri\\Cargo.toml` passed all 6 tests. Tests cover invalid endpoints/credentials, unsupported embeddings, explicit unreachable health and cancellation values, bearer-token redaction, unknown active providers, and settings round-trip persistence. `npm run lint`, `npm run typecheck`, `npm run build`, `cargo fmt --manifest-path src-tauri\\Cargo.toml --all -- --check`, and `cargo check --manifest-path src-tauri\\Cargo.toml` passed.

Incomplete work / blockers: no network transport or live provider smoke test is part of T02; those are T04. The settings writer uses a temporary file and replacement boundary; deeper vault conflict/migration behavior belongs to T03/T17.

Next concrete action: T03 — formalize character, memory, transcript, and operational-state storage with portable Markdown and safe persistence.

## 2026-09-11 — T03: character and transcript storage

Scope and outcome: formalized versioned character identity, memory records, transcript documents, and durable operational state. Added YAML-frontmatter Markdown serialization, an unambiguous length-delimited transcript turn format, stable UUID IDs, vault path confinement, load/create/validate behavior, atomic replacement with backup recovery, and the original Lyra sample character.

Files changed: `src-tauri/src/storage.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `examples/characters/lyra/character.md`, and the T03 status record.

Decisions added/superseded: none. Character identity remains separate from writable memory files; transcripts preserve roles, status, IDs, timestamps, and multiline Markdown content without relying on rendered text boundaries.

Verification and actual results: `cargo test --manifest-path src-tauri\\Cargo.toml` passed all 11 tests. The suite covers round-tripping character/memory/transcript/state data, bundled sample loading, invalid schemas, traversal and unsafe IDs, identity/memory separation, and a simulated interrupted write that leaves the previous valid file readable. `cargo check`, `cargo fmt --check`, frontend lint/type/build, and `git diff --check` also passed.

Incomplete work / blockers: provider transport, real HTTP streaming, model discovery, and conversation lifecycle remain unimplemented. The vault APIs are currently Rust-core APIs; T05 will connect them to chat commands and UI state.

Next concrete action: T04 — implement and mock-test Ollama and OpenAI-compatible provider connections, including fragmented streams and cancellation.

## 2026-09-11 — T04: provider connections

Scope and outcome: implemented asynchronous Ollama and OpenAI-compatible HTTP clients with provider-specific health/discovery paths, chat and embedding payloads, bearer-token handling, cancellation-aware streamed responses, fragmented NDJSON/SSE decoders, typed Tauri health/discovery/embedding commands, and deterministic protocol tests.

Files changed: `src-tauri/src/connections.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src/providers.ts`, and the T04 status record.

Decisions added/superseded: none. OpenAI-compatible embeddings remain capability-gated rather than assumed; the adapter uses `/v1/models`, `/v1/chat/completions`, and `/v1/embeddings`, while Ollama uses `/api/tags`, `/api/chat`, and `/api/embed`.

Verification and actual results: deterministic `cargo test --manifest-path src-tauri\\Cargo.toml` passed 16 tests. The explicitly ignored live test passed against the existing Ollama service on `127.0.0.1:11434`, discovering `llama3.2:latest` and receiving a real streamed reply in 18.84s. Frontend lint/type/build, Rust check/fmt, and `npx tauri build --no-bundle` passed.

Incomplete work / blockers: no llama.cpp or LM Studio server was running, so their live compatibility is not claimed. Port 8080 returned an unrelated Secret Garden HTML page. The provider stream service is ready for T05 orchestration; full UI event delivery and persisted turn lifecycle are not yet implemented.

Next concrete action: T05 — connect character/session selection, composer, streamed replies, cancellation/retry, and transcript persistence while snapshotting character/session IDs per request.

## 2026-09-11 — T05: conversation lifecycle

Scope and outcome: implemented the Rust conversation lifecycle service and connected it to the Tauri command boundary and a basic local composer. Each request snapshots character/provider/session/user-turn data, persists the user turn before inference, persists complete/failed/interrupted assistant turns, and supports idempotent delivery plus retry truncation without duplicate user messages.

Files changed: `src-tauri/src/conversation.rs`, `src-tauri/src/lib.rs`, `src/conversation.ts`, `src/ConversationPanel.tsx`, `src/App.tsx`, `src/App.css`, and the T05 status records.

Decisions added/superseded: none. The core accepts an injected transport so lifecycle behavior is testable without network access. Tauri send/retry commands currently create a per-request cancellation token; user-triggered stop control and incremental event delivery remain follow-up work.

Verification and actual results: `cargo test --manifest-path src-tauri\\Cargo.toml` passed 20 tests with one ignored live Ollama smoke test. `cargo check`, Rust fmt check, `npm run lint`, `npm run typecheck`, and `npm run build` all passed.

Incomplete work / blockers: the composer currently asks for a vault path, endpoint, and model directly; provider settings UI, character import/selection, incremental streamed rendering, and a stop button remain ahead. The next planned task is T06 prompt budgeting.

Next concrete action: implement deterministic prompt construction with a bounded token budget and tests, preserving system prompt and current user turn while trimming older context safely.

## 2026-09-11 — T06: budgeted character prompts

Scope and outcome: added deterministic prompt construction between the persisted conversation lifecycle and provider transport. The builder reserves output capacity, estimates input conservatively, assembles character rules/definition, optional scene context, recent complete history, and the current message, then trims oldest history when the input budget is exceeded.

Files changed: `src-tauri/src/prompt.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/lib.rs`, and the T06 status records.

Decisions added/superseded: none. The initial estimator uses roughly one token per three Unicode characters as a conservative fallback; model-specific tokenizers remain a later optimization. The actual request path is tested to include the selected character and serialize the current message once.

Verification and actual results: `cargo test --manifest-path src-tauri\\Cargo.toml` passed 24 tests with one ignored live Ollama smoke test. Prompt tests cover ordering, scene context, oversized base input, history trimming, reserved output capacity, multibyte estimation, and current-message de-duplication. `cargo check`, Rust fmt check, frontend lint/type/build, and `npx tauri build --no-bundle` all passed.

Incomplete work / blockers: the prompt builder currently has a fixed default budget and no retrieved memory context; T07 must add rebuildable Markdown-backed memory indexing. Streaming UI remains aggregated at the command boundary.

Next concrete action: implement memory CRUD, external-change detection, Markdown chunking, SQLite FTS5 indexing, full rebuild, and character isolation tests without making the index the source of truth.

## 2026-09-11 — T07: memory vault and lexical index

Scope and outcome: implemented a rebuildable SQLite FTS5 index over the existing Markdown memory records. Markdown remains the source of truth; the index fingerprints files, detects external edits, removes deleted records, excludes review-excluded memories, chunks long bodies, and can be rebuilt from scratch.

Files changed: `src-tauri/src/memory.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and the T07 status records.

Decisions added/superseded: none. The index lives inside each vault and is filtered by its character association; it is disposable and never the only copy of a memory. Update/rename operations reject an externally changed Markdown file until it is reloaded.

Verification and actual results: `cargo test --manifest-path src-tauri\\Cargo.toml` passed 27 tests with one ignored live Ollama smoke test. Memory tests cover create, external edit detection, rename, delete, chunking, exclusion, rebuild, and separate-vault isolation. `cargo check`, Rust fmt check, frontend lint/type/build, and `npx tauri build --no-bundle` all passed; the release includes bundled SQLite FTS5.

Incomplete work / blockers: lexical search is not yet connected to prompt context or the UI; salience/recency/pinned-memory selection belongs to T08. The index is synchronous and currently opened per vault operation.

Next concrete action: connect bounded lexical retrieval to prompt assembly with source labels, deduplication, salience/recency controls, pinned-memory budgets, and deterministic fixture tests.

## 2026-09-11 — T08: retrieved memory in chat

Scope and outcome: connected lexical memory search to bounded prompt construction. Retrieval deduplicates memory IDs, ranks pinned/salient/recent candidates, enforces total and pinned token budgets, carries source paths, and injects an explicitly labeled data-only context section into loopback-provider prompts.

Files changed: `src-tauri/src/memory.rs`, `src-tauri/src/retrieval.rs`, `src-tauri/src/prompt.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/lib.rs`, and the T08 status records.

Decisions added/superseded: remote-compatible endpoints do not receive vault memory by default; only loopback endpoints (`localhost`, `127.0.0.1`, or `::1`) get automatic memory context. This prevents accidental disclosure while leaving an explicit privacy-control extension for a future task.

Verification and actual results: `cargo test --manifest-path src-tauri\\Cargo.toml` passed 30 tests with one ignored live Ollama smoke test. Tests cover retrieval relevance, source labels, deduplication, salience/recency and pinned ordering, token budgets, prompt integration, and character isolation. `cargo check`, Rust fmt check, frontend lint/type/build, and `npx tauri build --no-bundle` all passed.

Incomplete work / blockers: memory browsing, per-turn source inspection, source navigation, and user-facing lock/exclude/delete actions belong to T09. Incremental streamed rendering and explicit remote-memory consent also remain ahead.

Next concrete action: build the memory browser/context inspector and expose source metadata without making the SQLite index the source of truth.

## 2026-09-11 — T09: memory review and context inspection

Outcome: completed T09. The application now exposes typed memory browse/search/upsert/delete commands, renders a Markdown-backed memory review panel, supports explicit accepted/needs-review/excluded status plus pin and lock controls, and shows the exact bounded retrieved sources used by each loopback-provider conversation turn. Excluded records remain browsable so users can restore them; the SQLite index still excludes them from retrieval.

Files changed: `src-tauri/src/storage.rs`, `src-tauri/src/memory.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/lib.rs`, `src/memory.ts`, `src/MemoryPanel.tsx`, `src/ConversationPanel.tsx`, `src/App.tsx`, `src/App.css`, `docs/STATUS.md`, and this handoff.

Verification: `cargo test --manifest-path src-tauri/Cargo.toml` passed with 31 tests and 1 ignored live-provider test; `npm run lint`, `npm run typecheck`, and `npm run build` passed; `npx tauri build --no-bundle` passed and produced `src-tauri/target/release/tz-chatter.exe`; `git diff --check` remains the final hygiene check.

Decision: memory records now carry a backward-compatible `locked` flag for explicit protection against future automatic changes. Remote providers still receive no vault-retrieved context by default; the context inspector therefore displays sources only for loopback retrieval.

Incomplete work / blockers: source navigation currently displays source paths and content in the inspector but does not yet open the Markdown file in an external editor. Extraction, reconciliation, review proposals, semantic ranking, initiative, portability, and packaging remain ahead.

Next action: start T10 by defining a validated persisted extraction-proposal queue; model output must remain untrusted data and cannot directly mutate Markdown memories.

## 2026-09-11 - T10: validated extraction proposals

Outcome: completed T10. Added a durable per-vault extraction queue in `.tz-chatter/state.sqlite3`. Completed transcript turns can be enqueued idempotently, user-chat priority is explicit, running jobs recover after restart, failures retry only up to a bounded maximum, and accepted model output becomes review-only proposals rather than direct Markdown writes.

Files changed: `src-tauri/src/extraction.rs`, `src-tauri/src/lib.rs`, `docs/STATUS.md`, and this handoff.

Verification: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` passed; `cargo test --manifest-path src-tauri/Cargo.toml` passed with 35 tests and 1 ignored live-provider test; `npm run lint`, `npm run typecheck`, `npm run build`, and `npx tauri build --no-bundle` passed.

Decision: the parser tolerates fenced or surrounding prose JSON but validates schema version, body length, confidence range, source-turn IDs, and evidence references. All proposals retain `needs_review`, including assistant-originated or inferred claims; no extraction path can promote a candidate to a vault memory.

Incomplete work / blockers: no provider-specific extraction call or review UI is wired yet; those belong to the worker integration and T12. The queue currently stores candidate proposals and is ready for T11 reconciliation.

Next action: start T11 by deduplicating proposals and committing accepted candidates atomically to Markdown before refreshing the rebuildable index.

## 2026-09-11 - T11: reconcile and commit memories

Outcome: completed T11. Proposal decisions are durable in `state.sqlite3`; accepted proposals now reconcile through a Markdown-first path, deduplicate across distinct extraction jobs, preserve contradiction/supersession links, respect locked memories, and suppress reconstruction of deleted memories. Tauri commands expose accept, reject, commit, and index repair operations.

Files changed: `src-tauri/src/extraction.rs`, `src-tauri/src/reconciliation.rs`, `src-tauri/src/lib.rs`, `src/memory.ts`, `docs/STATUS.md`, and this handoff.

Verification: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` passed; `cargo test --manifest-path src-tauri/Cargo.toml` passed with 40 tests and 1 ignored live-provider test; `npm run lint`, `npm run typecheck`, and `npm run build` passed; `npx tauri build --no-bundle` passed and produced `src-tauri/target/release/tz-chatter.exe`.

Decision: accepted candidates create new authoritative Markdown records rather than mutating existing memories. Existing records are only linked as duplicates, contradictions, or supersession targets, so newer user edits and locked files survive background reconciliation. The derived index can be rebuilt from the committed Markdown.

Incomplete work / blockers: proposal review UI and supporting transcript excerpts are still T12; provider-specific extraction scheduling is not yet connected to the chat lifecycle.

Next action: start T12 by exposing pending proposals and review decisions in the memory panel.

## 2026-09-11 - T12: memory review and correction

Outcome: completed T12. The memory panel now loads durable proposals with supporting evidence, allows editing candidate text, accepting, rejecting, and committing, and persists an auto-commit preference in local storage. Rejection removes a proposal from the queue without deleting its transcript; memory deletion remains a separate action with source suppression.

Files changed: `src-tauri/src/extraction.rs`, `src-tauri/src/lib.rs`, `src/memory.ts`, `src/MemoryPanel.tsx`, `src/App.css`, `docs/STATUS.md`, and this handoff.

Verification: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` passed; `cargo test --manifest-path src-tauri/Cargo.toml` passed with 41 tests and 1 ignored live-provider test; `npm run lint`, `npm run typecheck`, and `npm run build` passed; `npx tauri build --no-bundle` passed and produced `src-tauri/target/release/tz-chatter.exe`.

Decision: review edits update the durable proposal before acceptance; accepted proposals can commit through the existing Markdown-first reconciliation path, so corrected text is indexed for future retrieval. No proposal action erases transcript data.

Incomplete work / blockers: extraction jobs are not yet automatically scheduled from completed chat turns, and proposal review currently requires an explicit Refresh in the memory panel. Semantic indexing, initiative, portability, and final packaging remain ahead.

Next action: start T13 by adding versioned embedding metadata and a rebuildable vector index with lexical fallback.

## 2026-09-11 - T13: embeddings and versioned indexes

Outcome: completed T13. Added an isolated per-character SQLite embedding index with explicit provider/model/dimension/chunking/index metadata, canonical Markdown chunk fingerprints, direct cosine search, strict dimension validation, incompatible-space invalidation, and rebuild interruption recovery. Existing lexical retrieval remains independent and usable when embedding work fails or no model is configured.

Files changed: `src-tauri/src/embeddings.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/memory.rs`, `docs/STATUS.md`, and this handoff.

Verification: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` passed; `cargo test --manifest-path src-tauri/Cargo.toml` passed with 45 tests and 1 ignored live-provider test; `npm run lint`, `npm run typecheck`, and `npm run build` passed; `npx tauri build --no-bundle` passed and produced `src-tauri/target/release/tz-chatter.exe`.

Decision: an embedding index has one active space per character. Opening it with a changed provider, model, dimension, chunking version, or index version clears derived vectors before rebuilding; a durable `building` marker clears partial vectors after restart. Markdown and the lexical index remain the fallback source of chat retrieval.

Incomplete work / blockers: provider scheduling and UI controls for embedding rebuilds are not yet connected; hybrid ranking and evaluation belong to T14. No live embedding provider was added or claimed by these deterministic tests.

Next action: start T15 by defining durable, quiet-by-default initiative eligibility with cooldown and activity gates.

## Entry template

```text
## YYYY-MM-DD — task ID(s): short outcome

Scope and outcome:
Files changed:
Decisions added/superseded:
Verification and actual results:
Incomplete work / blockers:
Next concrete action:
```

## 2026-09-11 - T14: hybrid ranking and evaluation

Outcome: completed T14. Added an opt-in hybrid retrieval function that joins lexical candidates with versioned semantic candidates, applies entity overlap, reconciliation-link, salience, recency, and pinned signals, diversifies repeated memory types, and exposes machine-readable retrieval reasons and component scores through the conversation boundary.

Files changed: src-tauri/src/retrieval.rs, src-tauri/src/memory.rs, src-tauri/src/prompt.rs, src/conversation.ts, src/ConversationPanel.tsx, and docs/STATUS.md.

Verification: the deterministic evaluation fixture covers exact names, paraphrases, stale contradictory memories, distractors, foreign-character candidates, reason reporting, and token bounds. Representative output retained exact-name recall and semantic paraphrase recall, with lexical latency 995 us and hybrid latency 3166 us on the same small fixture. Full Rust tests passed: 46 tests, 45 passed and 1 ignored.

Decision: keep lexical retrieval as the default until provider scheduling, embedding freshness, and larger evaluation data justify enabling hybrid ranking by default. The hybrid path is explicit and bounded, so failed semantic work does not remove lexical fallback.

Incomplete work / blockers: no known blocker. The UI now displays reason labels for returned memories, but provider-specific background embedding scheduling and rebuild controls remain future work.

Next action: start T15 by defining durable, quiet-by-default initiative eligibility with cooldown and activity gates.

## 2026-09-12 - T15: persisted initiative eligibility

Outcome: completed T15. Added character-scoped initiative settings and counters in state.sqlite3, deterministic eligibility evaluation, Tauri commands, and a Settings panel. Initiative is disabled by default and cannot schedule while consent, activity, quiet-hour, cooldown, cap, unanswered, ignored-backoff, model/resource, or resume-suppression gates fail.

Files changed: src-tauri/src/initiative.rs, src-tauri/src/lib.rs, src/initiative.ts, src/InitiativePanel.tsx, src/App.tsx, and docs/STATUS.md.

Verification: full Rust tests passed with 50 passed and 1 ignored; initiative tests cover quiet-hour boundaries, disabled settings, frequency caps, restart persistence, unanswered state, ignored-message backoff, and sleep/resume catch-up suppression. npm lint, typecheck, build, and npx tauri build --no-bundle passed, producing src-tauri/target/release/tz-chatter.exe.

Decision: persist initiative settings and counters beside extraction state but key them by character. Expose record-activity, sent, ignored, and resume events for T16 to connect to real conversation and scheduling paths.

Incomplete work / blockers: delivery, topic selection, model silence, stale cancellation, tray, and notification behavior remain T16. No known blocker.

Next action: start T16 by implementing and testing the labeled initiative delivery path without fabricating user turns.

## 2026-09-12 - T16: labeled initiative delivery foundation

Outcome: continued T16. Added bounded selection of accepted open-thread memories, a typed initiative request/result contract, an internal initiative event prompt, explicit SILENCE classification, shared runtime cancellation/generation advancement, native tray show/quit behavior, a separate persisted notification preference, native notification dispatch, and delivery that persists an Initiative turn plus Assistant response without fabricating a User turn. Failed, canceled, or silent work remains out of the transcript.

Files changed: src-tauri/Cargo.toml, src-tauri/src/conversation.rs, src-tauri/src/initiative.rs, src-tauri/src/lib.rs, src/initiative.ts, src/conversation.ts, src/ConversationPanel.tsx, src/InitiativePanel.tsx, docs/STATUS.md, and docs/HANDOFF.md.

Verification: cargo fmt passed; full Rust tests passed with 54 passed and 1 ignored, including delivery, silence, stale-generation, newer-user-activity, open-topic filtering, notification persistence, and runtime replacement-cancellation regression tests. npm lint, typecheck, and build passed. npx tauri build --no-bundle passed and produced src-tauri/target/release/tz-chatter.exe. npm run tauri dev launched a responsive notification-enabled `tz-chatter` window. git diff --check passed.

Decision: initiative model output is treated as data. Only a completed non-silence response is persisted, with separate deterministic Initiative and Assistant turn IDs. Explicit stale checks prevent transport work when a newer user activity timestamp or generation is already known. The Tauri runtime owns one cancellation token per character/session key, and starting replacement work cancels the previous token before advancing its generation.

Incomplete work / blockers: tray/notification interaction has only been compile/runtime-launch verified rather than visually exercised. No known blocker.

Next action: visually verify tray and notification behavior on Windows, then move to T17 portability/recovery.

## 2026-09-12 - T17: validated portable character packs

Outcome: started T17 and implemented a directory-based portable pack workflow. Export includes canonical character Markdown, memories, transcripts, and durable operational state; it excludes rebuildable memory indexes, provider settings, credentials, and machine-specific paths. Import stages into a temporary directory, validates content and schema, rebuilds the search index, and only then moves into a new destination. Replacing an existing same-character vault preserves a backup; unrelated destinations are refused.

Files changed: src-tauri/src/portability.rs, src-tauri/src/lib.rs, src/portability.ts, src/PortabilityPanel.tsx, src/App.tsx, docs/STATUS.md, and docs/HANDOFF.md.

Verification: portability tests pass for canonical/durable-state round-trip, legacy manifest migration, traversal and unknown-schema rejection, corrupt source-index recovery, unrelated-vault protection, and same-character replacement backup preservation. Full Rust suite passed with 59 passed and 1 ignored. npm lint, typecheck, build, git diff --check, and npx tauri build --no-bundle passed; release executable produced at src-tauri/target/release/tz-chatter.exe.

Decision: use a validated directory pack rather than a proprietary archive for the first portable workflow. Pack paths are a fixed relative allowlist. Rebuildable indexes are never copied; restore regenerates them from Markdown. Durable state.sqlite3 and state.json are preserved because review/suppression/scheduler state belongs to the character workflow and contains no provider credentials.

Incomplete work / blockers: broader backup/restore failure injection and runtime recovery validation remain. T16 tray/notification visual interaction remains unavailable because the Computer Use helper could not initialize. No known implementation blocker.

Next action: add explicit backup/restore failure-recovery tests and then begin T18 Windows journey, provider, accessibility, and packaging validation.

## 2026-09-12 - T18: Windows release validation in progress

Outcome: advanced T18 validation. The ignored live Ollama smoke test passed against the local service in 27.59s. The full bundled Tauri build produced both MSI and NSIS installers, and the packaged executable launched with a responsive `tz-chatter` window.

Files changed: README.md, docs/STATUS.md, and docs/HANDOFF.md; release outputs are under src-tauri/target/release/bundle/.

Verification: full Rust suite passed with 59 passed and 1 ignored; live Ollama smoke passed; frontend lint, typecheck, and build passed after the final ARIA-label edits; a static UI audit found explicit button types and labels/ARIA for controls; the final `npx tauri build` produced `src-tauri/target/release/bundle/msi/tz-chatter_0.1.0_x64_en-US.msi` (4,378,624 bytes) and `src-tauri/target/release/bundle/nsis/tz-chatter_0.1.0_x64-setup.exe` (3,219,523 bytes); the final executable launch reported `MainWindowTitle=tz-chatter` and `Responding=True`. Representative retrieval measured lexical 1,089 us and hybrid 3,511 us; the persisted-vault round-trip test process took 562.10 ms wall time.

Decision: retain the current release support claim as local Ollama verified plus protocol-level Ollama/OpenAI-compatible coverage. OpenAI-compatible live support is not claimed until a real llama.cpp or LM Studio endpoint is available. The visual tray/notification and accessibility inspection remains unverified because the Computer Use helper failed to initialize twice after reset.

Incomplete work / blockers: keyboard/accessibility and tray/notification visual checks are not complete. No implementation blocker; the remaining gap is environment/tool availability and live provider access.

Next action: superseded by the final restart-resume and packaging validation entry below; retain T18 in progress for the documented visual-validation and live-provider limitations.

## 2026-09-12 - T18: restart-resume journey and final packaging completed

Outcome: closed a release-journey gap found during completion audit. The conversation boundary now loads the vault character and last persisted transcript session, send/retry/initiative flows persist the last opened session, and the UI remembers the selected vault path, auto-loads it on launch, and exposes an explicit Load conversation action. Landing-page actions now navigate to real views, active navigation exposes `aria-current`, and keyboard focus has a visible `:focus-visible` treatment. First-run and troubleshooting documentation now describe the actual Windows workflow and unsigned package limitations.

Files changed: src-tauri/src/conversation.rs, src-tauri/src/lib.rs, src/conversation.ts, src/ConversationPanel.tsx, src/App.tsx, src/App.css, README.md, docs/STATUS.md, and docs/HANDOFF.md.

Verification: focused resume regression passed; full Rust suite passed with 60 passed and 1 ignored; the live Ollama smoke test passed in 2.50s; npm lint, typecheck, and build passed after the final UI edits; static audit found explicit button types, active navigation semantics, visible keyboard focus, and control labels/ARIA; final `npx tauri build` produced the MSI (4,382,720 bytes) and NSIS (3,222,239 bytes) artifacts; the final executable launched with `MainWindowTitle=tz-chatter` and `Responding=True`; `git diff --check` passed.

Decision: use the existing durable `OperationalState.last_opened_session_id` as the resume pointer and load the canonical `character.md` from the selected vault. A missing transcript pointer starts a new session; a transcript belonging to another character is rejected.

Incomplete work / blockers: visual tray/notification/accessibility interaction remains unavailable because the Windows Computer Use helper cannot initialize, and live llama.cpp/LM Studio coverage remains unavailable. The package is local and unsigned; no external publishing was requested.

Next action: retain T18 in progress for the documented visual/tool and live non-Ollama provider limitations; rerun those checks if the environment gains the required helper or endpoint.

## 2026-09-12 - T10/T18: automatic extraction worker connected

Outcome: connected completed chat replies to the existing durable extraction pipeline. A completed assistant turn is now enqueued with its user/assistant source IDs; the Tauri runtime claims one pending job, builds a bounded data-labelled extraction request for the selected provider/model, accepts only parser-validated output, and leaves malformed, failed, or cancelled work retryable without changing canonical Markdown. Initiative turns remain excluded from automatic factual extraction.

Files changed: src-tauri/src/extraction.rs, src-tauri/src/conversation.rs, src-tauri/src/lib.rs, src/MemoryPanel.tsx, docs/STATUS.md, docs/HANDOFF.md, and README.md.

Verification: full Rust suite passed with 61 passed and 1 ignored; focused conversation enqueue and extraction prompt tests passed; Rust format/check passed; npm lint, typecheck, and build passed; live Ollama smoke passed in 2.50s; final MSI (4,403,200 bytes) and NSIS (3,237,938 bytes) packages built and the executable launched responsively.

Decision: background extraction uses only the provider configuration explicitly selected for the completed chat request. The chat result is not failed if the queue cannot be opened, and model output remains review-only until the existing reconciliation controls commit it.

Incomplete work / blockers: no live llama.cpp/LM Studio endpoint is available, and visual Windows tray/notification/accessibility interaction remains unavailable because the Computer Use helper cannot initialize. The worker is bounded to one claim per completed chat command; pending jobs remain durable for a later trigger or restart recovery.

Next action: retain T18 in progress for the documented visual/tool and live-provider limitations; consider adding an explicit queued-job status/retry control if the product needs manual background-work observability.

## 2026-09-12 - T18: provider controls connected

Outcome: connected the existing provider contract to the Conversation UI. Users can select Ollama or OpenAI-compatible transport, edit endpoint/model, keep an optional bearer token in app configuration, save settings outside the character vault, check reachability, and discover chat-capable models. The selected provider configuration is still passed directly to chat and background extraction.

Files changed: src/ConversationPanel.tsx, src/App.css, README.md, docs/STATUS.md, and docs/HANDOFF.md.

Verification: npm lint, typecheck, and production build passed; the final Tauri build produced MSI (4,403,200 bytes) and NSIS (3,237,648 bytes); the rebuilt executable launched with MainWindowTitle=tz-chatter and Responding=True; git diff --check passed.

Decision: provider settings remain app-scoped and are never included in portable character packs. Live verification remains limited to Ollama; OpenAI-compatible behavior is covered by deterministic protocol tests until a compatible local endpoint is available.

Incomplete work / blockers: visual Windows tray/notification/accessibility interaction remains unavailable because the Computer Use helper cannot initialize; no live llama.cpp/LM Studio endpoint is available.

Next action: retain T18 in progress for the documented visual/tool and live-provider limitations.

## 2026-09-12 - T16: initiative scheduler connected

Outcome: connected the durable initiative eligibility state to a cancellable in-app scheduler. Saving enabled settings from the Initiative panel starts a per-character/session 30-second loop; disabling and saving stops it. User chat activity is persisted before work begins and cancels overlapping initiative generations. Ignored and model-silenced outcomes update durable cooldown/backoff state, while delivered turns remain labeled initiative messages and optional Windows notifications.

Files changed: src-tauri/Cargo.toml, src-tauri/src/initiative.rs, src-tauri/src/lib.rs, src/initiative.ts, src/InitiativePanel.tsx, README.md, docs/STATUS.md, and docs/HANDOFF.md.

Verification: cargo fmt check passed; full Rust tests passed with 61 passed and 1 ignored; live Ollama smoke passed in 14.76s; npm lint, typecheck, and build passed. The final bundled release rebuild is still required after this UI integration. Computer Use could not initialize, so visual tray/notification interaction remains unverified.

Decision: keep scheduler ownership in the Rust core and expose only typed start/stop commands to the UI. The scheduler captures the explicitly saved provider configuration at start, yields to active chat work, and stops with the Tauri runtime.

Incomplete work / blockers: release packaging and executable launch need to be rerun after the scheduler changes; live non-Ollama and visual Windows interaction checks remain unavailable.

Next action: rebuild MSI/NSIS, launch the packaged executable, run git diff --check, then retain T18 in progress for the documented environment limitations.

## 2026-09-12 - T18: scheduler-inclusive Windows package

Outcome: rebuilt the complete Windows release after connecting the initiative scheduler UI. The packaged executable launched with MainWindowTitle=tz-chatter and Responding=True.

Verification: npx tauri build passed; MSI is 4,415,488 bytes at src-tauri/target/release/bundle/msi/tz-chatter_0.1.0_x64_en-US.msi and NSIS is 3,247,998 bytes at src-tauri/target/release/bundle/nsis/tz-chatter_0.1.0_x64-setup.exe. Scheduler source audit and git diff --check passed.

Incomplete work / blockers: the Computer Use helper was retried after reading its current guidance and still failed during initialization with `windows sandbox failed: helper_unknown_error: setup refresh had errors`, so visual tray/notification/accessibility interaction is not verified; no live llama.cpp/LM Studio endpoint is available. Packages are local and unsigned.

Next action: retain T18 in progress for those environment-limited checks; the next executable step is to rerun visual interaction and live non-Ollama provider checks if those become available.

## 2026-09-12 - T16/T18: active-session scheduler correction

Outcome: corrected a lifecycle gap found during audit. Initiative now uses the active Conversation character/session identity, so spontaneous turns share the user transcript and runtime cancellation key. Loading another vault stops the prior scheduler; enabled initiative settings are discovered and restarted during remembered-vault resume; retry actions record user activity; persisted settings are loaded when reopening Settings.

Files changed: src/activeSession.ts, src/ConversationPanel.tsx, src/InitiativePanel.tsx, src-tauri/src/lib.rs, README.md, docs/STATUS.md, and docs/HANDOFF.md.

Verification: full Rust suite passed with 62 passed and 1 ignored; added runtime scheduler replacement/child-cancellation coverage; live Ollama smoke passed in 2.56s; Rust format check, frontend lint/typecheck/build, scheduler source audit, and git diff --check passed. The corrected npx tauri build produced MSI (4,415,488 bytes) and NSIS (3,247,932 bytes); the packaged executable launched with MainWindowTitle=tz-chatter and Responding=True, then the exact test process was stopped.

Incomplete work / blockers: visual tray/notification/accessibility interaction remains unavailable because the Computer Use helper cannot initialize; no live llama.cpp/LM Studio endpoint is available. Packages are local and unsigned.

Next action: retain T18 in progress for the environment-limited visual and live non-Ollama checks; all available build and runtime checks are current.

## 2026-09-12 - T16: provider-save scheduler refresh

Outcome: saving provider settings now refreshes an enabled initiative scheduler for the active character/session, ensuring spontaneous work uses the newly selected endpoint and model instead of a stale configuration.

Files changed: src/ConversationPanel.tsx, docs/STATUS.md, and docs/HANDOFF.md.

Verification: frontend lint, typecheck, and build passed; the final npx tauri build produced MSI (4,415,488 bytes) and NSIS (3,248,865 bytes); the packaged executable launched with MainWindowTitle=tz-chatter and Responding=True, then the exact test process was stopped.

Incomplete work / blockers: visual tray/notification/accessibility interaction remains unavailable because the Computer Use helper failed initialization; live llama.cpp/LM Studio coverage remains unavailable. Packages are local and unsigned.

Next action: retain T18 in progress for the documented environment limitations.

## 2026-09-12 - T18: live OpenAI-compatible transport smoke

Outcome: added a dedicated ignored smoke test for the OpenAI-compatible provider path against Ollama’s `/v1` endpoint. The test performs health, model discovery, and streamed chat through the OpenAI-compatible request/response format. This validates the transport implementation live without claiming a live llama.cpp or LM Studio integration.

Files changed: src-tauri/src/connections.rs, docs/STATUS.md, and docs/HANDOFF.md.

Verification: the new live smoke test passed in 2.49s; the full Rust suite passed with 62 passed and 2 ignored; `cargo test --manifest-path src-tauri/Cargo.toml -- --ignored --nocapture` passed both live provider smoke tests together in 0.62s; Rust formatting passed. Native Ollama remains live-verified separately in 2.56s.

Incomplete work / blockers: visual Windows tray/notification/accessibility interaction remains unavailable because the Computer Use helper failed initialization; live llama.cpp/LM Studio coverage remains unavailable.

Next action: retain T18 in progress with the release support claim limited to live Ollama plus live Ollama OpenAI-compatible transport and deterministic third-party protocol coverage.

## 2026-09-12 - T19: completion-audit findings A01-A15

Scope and outcome: finished the leftover ChatGPT/Codex T19 issue list in `docs/AUDIT.md`. Existing remediations were kept. Remaining gaps closed with canonical search-record edits, vault/character command validation, composer load gating, active-character UI identity, hybrid retrieval with lexical fallback, incremental stream events, a draining resume extraction worker that yields to chat, natural-language lexical retrieval, local-offset quiet hours and resume suppression, one labelled initiative turn plus `initiative-delivered` UI updates, Context-nav removal and inspector source/budget details, close-to-tray, real newline memory separators, strict Clippy, context-limit prompt retry, and conflict-checked memory-type moves.

Files changed: `src-tauri/src/lib.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/prompt.rs`, `src-tauri/src/retrieval.rs`, `src/ConversationPanel.tsx`, `src/MemoryPanel.tsx`, `src/PortabilityPanel.tsx`, `docs/AUDIT.md`, `docs/STATUS.md`, `docs/HANDOFF.md`, plus the previously untracked application tree committed as `6a08a51` for A13.

Decisions added/superseded: none.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` — 79 passed, 2 ignored; `cargo fmt --all -- --check` passed; `cargo clippy --all-targets -- -D warnings` passed; `npm run lint`, `npm run typecheck`, `npm run build` passed; `npx tauri build` produced MSI 4,485,120 bytes and NSIS 3,294,811 bytes; packaged `tz-chatter.exe` launched twice with `MainWindowTitle=tz-chatter` and `Responding=True`, then those processes were stopped.

Incomplete work / blockers: T16/T18 visual tray/notification/keyboard Computer Use interaction remains unavailable (no helper in this session). Live llama.cpp/LM Studio coverage remains unavailable. Packages are local and unsigned.

Next concrete action: retain T16/T18 in progress for those environment-limited checks; T19 is complete.

## 2026-09-12 — T20: chat-program layout

Scope and outcome: the previous working tree was already clean on `8abe734`; only untracked `.vscode/extensions.json` was present and was left untracked. Replaced the landing-page shell and scattered provider/initiative/portability forms with a Discord/Slack-style layout. Chat is the default surface: a sidebar of saved sessions, a full-height transcript, and a composer. Provider, initiative, and vault/portability live in a Settings overlay. Memories uses the loaded character instead of duplicate vault fields. Added `conversation_list_sessions`, `conversation_open_session`, and `conversation_start_session`.

Files changed: `src/App.tsx`, `src/App.css`, `src/ConversationPanel.tsx`, `src/MemoryPanel.tsx`, `src/InitiativePanel.tsx`, `src/PortabilityPanel.tsx`, `src/ProviderPanel.tsx`, `src/SettingsPanel.tsx`, `src/activeSession.ts`, `src/conversation.ts`, `src-tauri/src/conversation.rs`, `src-tauri/src/storage.rs`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `README.md`, `docs/PLAN.md`, `docs/STATUS.md`, and this handoff.

Decisions added/superseded: none. One-active-character remains; sessions are listed from `chats/*.md` inside the loaded vault.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` — 80 passed, 2 ignored, including `lists_opens_and_starts_sessions_without_mixing_characters`; `cargo fmt --all -- --check` passed; `npm run lint`, `npm run typecheck`, and `npm run build` passed. Native window / Computer Use visual inspection was not run in this session.

Incomplete work / blockers: T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: retain T16/T18 in progress; T20 is complete.

## 2026-09-12 — Sample vault memories for first-run testing

Scope and outcome: filled the existing Lyra example vault so it can be opened as a default test character. Added six Markdown memories (people, episodic, semantic, relationships, open thread) and a three-turn welcome transcript. The search index remains rebuildable and is not stored in the sample.

Files changed: `examples/characters/lyra/memories/**`, `examples/characters/lyra/chats/session-welcome.md`, `src-tauri/src/storage.rs` (load test), `README.md`, `docs/STATUS.md`, and this handoff.

Verification: `cargo test --manifest-path src-tauri/Cargo.toml bundled_sample` passed both sample-character tests.

Next concrete action: open `examples/characters/lyra` in the app; click **session-welcome** or ask about Mina / Markdown notes to exercise retrieval.

## 2026-09-12 — T21: character library and application prompt

Scope and outcome: added a Characters primary view that lists known vaults, creates new portable character folders, edits author-controlled identity, and switches the active character. Settings gained a Prompt section for global application rules stored in app config. Chat and initiative requests now prepend those rules before character identity and memories; empty saved text omits the extra message. Opening a vault also adds it to the library. Missing vaults stay listed with an error. Creating a character refuses to overwrite an existing `character.md`. Extraction still cannot edit character identity.

Files changed: `src/App.tsx`, `src/App.css`, `src/CharacterPanel.tsx`, `src/characters.ts`, `src/PromptPanel.tsx`, `src/SettingsPanel.tsx`, `src/ConversationPanel.tsx`, `src/conversation.ts`, `src/initiative.ts`, `src-tauri/src/characters.rs`, `src-tauri/src/prompt.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/initiative.rs`, `src-tauri/src/lib.rs`, `docs/PLAN.md`, `docs/STATUS.md`, `docs/DECISIONS.md`, `docs/PROJECT.md`, `README.md`, and this handoff.

Decisions added/superseded: ADR-008.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` — 90 passed, 2 ignored; `cargo fmt --all -- --check` passed; `cargo clippy --all-targets -- -D warnings` passed; `npm run lint`, `npm run typecheck`, and `npm run build` passed. Native window / Computer Use visual inspection was not run in this session.

Incomplete work / blockers: T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: retain T16/T18 in progress; T21 is complete. Open Characters to create or add a vault, and Settings → Prompt to edit the application rules.

## 2026-09-12 — T22: populate chat model list from the provider

Scope and outcome: Settings → Provider no longer uses a free-text chat model field with a hidden datalist. The panel queries `provider_discover` on load and whenever provider kind, endpoint, or bearer token change, then fills a select with chat-capable models. A saved model that the provider does not currently list remains selectable. Other… reveals a text field for a custom name. If discovery fails or returns nothing, the original text field is shown. The previous Discover models button is now Refresh models.

Files changed: `src/ProviderPanel.tsx`, `docs/PLAN.md`, `docs/STATUS.md`, and this handoff.

Decisions added/superseded: none. This uses the existing T04 discovery contract; Ollama still reports every tagged model as chat-capable.

Verification and actual results: `npm run lint`, `npm run typecheck`, and `npm run build` passed. Native window interaction and a live Ollama discovery click-through were not run in this session.

Incomplete work / blockers: T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited. Embedding model remains a free-text field.

Next concrete action: retain T16/T18 in progress; T22 is complete. Open Settings → Provider with a reachable local provider to confirm the chat model select fills.

## 2026-09-12 — T22 follow-up: Other… is a single custom field

Scope and outcome: selecting Other… no longer leaves the model dropdown visible while adding a text field. Custom entry replaces the select with one input. Choose from provider list returns to the discovered-model dropdown.

Files changed: `src/ProviderPanel.tsx`, `src/App.css`, `docs/STATUS.md`, and this handoff.

Verification: `npm run lint` and `npm run typecheck` passed.

Next concrete action: retain T16/T18 in progress.

## 2026-09-12 — T23: first-class LM Studio provider settings

Scope and outcome: Settings → Provider now has an **LM Studio** option alongside Ollama and generic OpenAI-compatible. Selecting it uses id `lm-studio-local` and default endpoint `http://127.0.0.1:1234/v1`. Health, model discovery, streamed chat, and embeddings go through the existing OpenAI-compatible `/v1` transport (`/v1/models`, `/v1/chat/completions`, `/v1/embeddings`). Previously saved `open_ai_compatible` settings still deserialize. Newly selected OpenAI-compatible defaults to `http://127.0.0.1:8080/v1` so it is no longer confused with LM Studio’s port.

Files changed: `src-tauri/src/providers.rs`, `src-tauri/src/connections.rs`, `src/providers.ts`, `src/ProviderPanel.tsx`, `README.md`, `docs/PLAN.md`, `docs/STATUS.md`, and this handoff.

Decisions added/superseded: none. This implements ADR-002’s LM Studio path as a named settings kind rather than a new protocol.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` — 92 passed, 2 ignored; `cargo fmt --all -- --check` passed; `cargo clippy --all-targets -- -D warnings` passed; `npm run lint`, `npm run typecheck`, and `npm run build` passed. A probe of `http://127.0.0.1:1234/v1/models` timed out, so live LM Studio is not claimed.

Incomplete work / blockers: T16/T18 visual tray/keyboard Computer Use and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: retain T16/T18 in progress. Start LM Studio’s Developer server and choose **LM Studio** in Settings → Provider to exercise discovery against a real model list.

## 2026-09-12 — Possible later features recorded

Scope and outcome: recorded the post-T23 product-ideation backlog as unscheduled possible features. Highest-leverage candidates: memory inbox, remember-this-from-a-turn, user persona and scene notes, open-vault-as-files plus portraits, session names/search/archive, edit/regenerate/continue, per-character model and sampling, and a first-run provider coach with explicit remote-memory consent. On-mission follow-ups: embedding rebuild and hybrid default, contradiction/supersession review, keyboard-first chat, and macOS/Linux verification. Deferred items (voice, avatars, group chat, marketplace, cloud sync, fine-tuning, arbitrary tools, bundled downloads, always-on after Quit) stay deferred; engine launch may be revisited only as a user-visible action.

Files changed: `docs/PROJECT.md`, `docs/PLAN.md`, `docs/STATUS.md`, `README.md`, and this handoff.

Decisions added/superseded: none. These ideas are not accepted implementation scope. The plan forbids adding T24+ until the user promotes a specific item.

Verification and actual results: documentation-only; no application checks.

Incomplete work / blockers: none for this documentation change. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: retain T16/T18 in progress. Promote the suggested cluster (memory inbox, remember-this, open-vault-as-files) into the plan only after an explicit user decision.

## 2026-09-12 — Self-maintaining hybrid memory spec drafted

Scope and outcome: wrote the design agreed in discussion. Markdown files remain canonical. The model proposes; the app validates and auto-writes user-stated, event, and character/world candidates. Inferred guesses stay inbox-only. Origin labels are reclassified from transcript evidence (user_stated needs a user-turn quote). Hybrid retrieval becomes the default when an embedding model is configured, with visible lexical fallback. Vectors stay a rebuildable SQLite index, not an external database. Chat-chrome inbox badge and Remember-this are explicitly out of this spec.

Files changed: `docs/specs/2026-09-12-self-maintaining-hybrid-memory-design.md`, `docs/STATUS.md`, `README.md`, and this handoff.

Decisions added/superseded: none yet. ADR-004/ADR-005 and `docs/PROJECT.md` update only after the user accepts the spec and it is promoted into the plan.

Verification and actual results: documentation-only; no application checks. Spec self-review removed origin-label ambiguity and scoped inbox chrome out of the first implementation.

Incomplete work / blockers: spec is awaiting user review. Do not implement. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: user reviews `docs/specs/2026-09-12-self-maintaining-hybrid-memory-design.md`. On acceptance, add plan tasks and supersede the lexical-default / all-needs-review defaults.

## 2026-09-12 — Milestone 8 planned (self-maintaining hybrid memory)

Scope and outcome: the user confirmed the product loop is “just chat; memories accumulate.” The spec is accepted. ADR-009 supersedes ADR-004’s lexical product default and ADR-005’s all-proposals-need-review default. `docs/PROJECT.md` now specifies auto-write of validated facts as new Markdown files, inferred inbox-only, and hybrid default with lexical fallback. PLAN tasks: T24 auto-write, T25 hybrid default + background embed. Executor plan is `docs/specs/2026-09-12-self-maintaining-hybrid-memory-plan.md`. No application code.

Files changed: `docs/PLAN.md`, `docs/PROJECT.md`, `docs/DECISIONS.md`, `docs/STATUS.md`, `README.md`, `docs/specs/2026-09-12-self-maintaining-hybrid-memory-design.md`, `docs/specs/2026-09-12-self-maintaining-hybrid-memory-plan.md`, and this handoff.

Decisions added/superseded: ADR-009 accepted. ADR-004 and ADR-005 defaults superseded as recorded in `docs/DECISIONS.md`.

Verification and actual results: documentation-only.

Incomplete work / blockers: T24 and T25 are `todo`. T16/T18 visual/live-provider checks remain environment-limited.

Next concrete action: start T24. Write failing `effective_origin` / auto-commit tests, then wire `process_extraction_queue` to `ReconciliationService` per the executor plan.

## 2026-09-12 — T24: auto-write validated memories

Scope and outcome: after a successful extraction job, each proposal’s origin is classified against the transcript. `user_stated` without a user-turn evidence quote is treated as inferred. Validated user-stated, conversation-event, and character-fact proposals are auto-accepted and committed as **new** Markdown files through existing `MemoryStore::create` / `commit_accepted`. Inferred guesses stay `needs_review` and are not indexed. Duplicate, suppression, supersession-link, and lock behavior is unchanged; `CommitResult::Locked` reverts the proposal to `needs_review`. Workers do not mutate existing memory bodies. `superseded_targets` was not implemented (T25).

Files changed: `src-tauri/src/extraction.rs`, `src-tauri/src/reconciliation.rs`, `src-tauri/src/lib.rs`, `docs/STATUS.md`, and this handoff.

Decisions added/superseded: none. Follows ADR-009.

Verification and actual results: TDD red on missing `effective_origin` / `auto_commit`; green after wiring. `cargo test --manifest-path src-tauri/Cargo.toml` PASS: 101 passed, 2 ignored. `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` PASS. `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` PASS.

Incomplete work / blockers: T25 not started. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: start T25 — hybrid retrieval by default, lexical fallback without embedding on the chat path, background embed worker, skip superseded targets.

## 2026-09-12 — T25: hybrid retrieval by default

Scope and outcome: hybrid retrieval is used when `use_hybrid_retrieval` is on and the active provider has a non-empty embedding model and the per-character vector index is ready. A missing or stale index does not embed on the chat path: send stays lexical, records `Embedding index is rebuilding; lexical fallback used.`, and schedules background job `embedding:{character.id}`. That worker takes `model_gate`, yields, honors cancellation (leaving the index not-ready), and rebuilds via `ProviderClient::embed` / `rebuild_precomputed`. The same job is scheduled after a successful T24 auto-commit when the extraction provider has an embedding model. `ExtractionQueue::superseded_targets` skips old IDs that are the target of a `supersedes` link. UI hybrid defaults on (`localStorage !== "false"`). README first-run states chatting saves supported facts without Accept.

Files changed: `src-tauri/src/extraction.rs`, `src-tauri/src/retrieval.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/embeddings.rs`, `src-tauri/src/lib.rs`, `src/ProviderPanel.tsx`, `src/ConversationPanel.tsx`, `README.md`, `docs/STATUS.md`, and this handoff.

Decisions added/superseded: none. Follows ADR-009. Query embedding still runs on the chat path only when the index is already ready (required for semantic search); index rebuild does not.

Verification and actual results: TDD red on missing `superseded_targets` / skip-ID signatures; green after gating and worker. `cargo test --manifest-path src-tauri/Cargo.toml` PASS: 106 passed, 2 ignored. `cargo fmt --all -- --check` PASS. `cargo clippy --all-targets -- -D warnings` PASS. `npm run lint` PASS. `npm run typecheck` PASS.

Incomplete work / blockers: live embedding quality against a reachable embedding model was not measured. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-12 — Milestone 8 important-finding fixes

Scope and outcome: `ReconciliationService::auto_commit` restores `NeedsReview` when `commit_accepted` returns `Err` after an Accepted mark, so a failed write stays in the inbox instead of disappearing as orphan `Accepted`. `process_extraction_queue` no longer aborts the drain on MemoryStore open, reclassify, or auto-commit errors: that item stays `needs_review`, remaining jobs keep processing. `process_embedding_rebuild` takes a `ChatTransport` so tests can fake-embed; production still uses `ProviderClient`.

Files changed: `src-tauri/src/reconciliation.rs`, `src-tauri/src/lib.rs`, `docs/STATUS.md`, this handoff, and `.superpowers/sdd/2026-09-12-self-maintaining-hybrid-memory-plan/task-final-fix-report.md`.

Decisions added/superseded: none. No new plan IDs.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` PASS: 110 passed, 2 ignored. `cargo fmt --all -- --check` PASS. `cargo clippy --all-targets -- -D warnings` PASS. Covering tests: `auto_commit_error_restores_needs_review`, `extraction_worker_continues_after_auto_commit_error`, `embedding_rebuild_worker_clears_needs_rebuild_when_chunks_exist`, `embedding_rebuild_cancel_before_finish_leaves_index_not_ready`.

Incomplete work / blockers: live embedding quality not claimed. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-12 — T26: user persona and reusable locals

Scope and outcome: first-class portable Markdown for who the user is and where the scene is, outside `character.md`. Vault files: `persona.md`, `scene.md` (character default local id), and `locals/<id>.md`. A local must exist in `locals/` before the character default or a session can select it. Session frontmatter `local` is a live link: missing follows the default, empty clears the scene, `cafe` reads `locals/cafe.md` at send time. Prompt order is application rules, character identity, labeled user persona, labeled current scene, memories, history, current message; empty sections are omitted. Extraction still writes only `memories/`. Packs include persona, scene, and locals. Characters edits persona, locals, and the default; chat picks Character default / None / an existing local, or creates a local then selects it.

Files changed: `src-tauri/src/scene.rs`, `src-tauri/src/storage.rs`, `src-tauri/src/prompt.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/portability.rs`, `src-tauri/src/characters.rs`, `src-tauri/src/lib.rs`, `src/scene.ts`, `src/conversation.ts`, `src/CharacterPanel.tsx`, `src/ConversationPanel.tsx`, `src/PortabilityPanel.tsx`, `src/App.css`, `examples/characters/lyra/persona.md`, `examples/characters/lyra/scene.md`, `examples/characters/lyra/locals/cafe.md`, `README.md`, `docs/PROJECT.md`, `docs/PLAN.md`, `docs/DECISIONS.md`, `docs/STATUS.md`, and this handoff.

Decisions added/superseded: ADR-010 accepted.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` PASS 120 passed, 2 ignored. `cargo fmt --all -- --check` PASS. `cargo clippy --all-targets -- -D warnings` PASS. `npm run lint` PASS. `npm run typecheck` PASS. `npm run build` PASS. Native window interaction was not re-run.

Incomplete work / blockers: none for T26. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-12 — T26 follow-up: persona and locals in character add/edit

Scope and outcome: Characters New, Add, and Save now treat persona and locals as part of building a character, not a side editor. Creating a vault writes empty `persona.md` and `scene.md` next to `character.md` and `locals/`. The New form collects user persona and an optional first local (used as the default scene). Add loads those files into the editor. Save character files writes identity, persona, scene default, and the local currently in the form.

Files changed: `src-tauri/src/characters.rs`, `src/CharacterPanel.tsx`, `src/App.css`, `README.md`, `docs/STATUS.md`, and this handoff.

Verification and actual results: create-character test asserts persona.md and scene.md exist; Clippy, fmt check, frontend lint/typecheck/build passed. Native window not re-run.

Incomplete work / blockers: none for this follow-up. T16/T18 remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-12 — T27: character portraits

Scope and outcome: portable `assets/portrait.png` renders in the Characters library list, the chat sidebar active-character card, and the identity editor. Missing files fall back to initials. The editor can choose, replace, drag-drop, or remove a PNG; dropping `assets/portrait.png` into the vault folder is detected on the next library list or window focus. New vaults scaffold an empty `assets/` directory. Writes stay confined to the selected vault; non-PNG and files larger than 5 MB are rejected. Packs include a valid portrait when present. Open-folder / Obsidian remain unscheduled.

Files changed: `src-tauri/src/characters.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/portability.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src/characters.ts`, `src/CharacterPanel.tsx`, `src/CharacterPortrait.tsx`, `src/App.tsx`, `src/App.css`, `src/activeSession.ts`, `README.md`, `docs/PROJECT.md`, `docs/PLAN.md`, `docs/DECISIONS.md`, `docs/STATUS.md`, `docs/Future-Growth-Notes.md`, and this handoff.

Decisions added/superseded: ADR-011 accepted.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` PASS 124 passed, 2 ignored. `cargo fmt --all -- --check` PASS. `cargo clippy --all-targets -- -D warnings` PASS. `npm run lint` PASS. `npm run typecheck` PASS. `npm run build` PASS. Native window interaction was not re-run.

Incomplete work / blockers: none for T27. JPEG/WebP conversion, animated avatars, and open-vault-as-files remain unscheduled. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-12 — T28: session names, search, and archive

Scope and outcome: conversation sidebar titles, transcript search, and archive without deleting Markdown. Transcript YAML gained optional `title` and `archived` (schema still 1; missing fields load as untitled/active). Filenames stay `{session_id}.md` in `chats/`. The first user turn auto-fills an empty title on the next conversation save and never overwrites a rename. Archive is `archived: true` on the same file. Default `list_sessions` hides archived chats; `list_sessions_filtered(..., true)` and search include them. Search is a case-insensitive substring over title and turn bodies for the loaded character, with a snippet on hits. No transcript SQLite index. Packs already include `chats/*.md`. The sidebar shows titles, a search field, rename, archive/unarchive, and a Show archived control.

Files changed: `src-tauri/src/storage.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/prompt.rs`, `src-tauri/src/portability.rs`, `src/conversation.ts`, `src/activeSession.ts`, `src/App.tsx`, `src/ConversationPanel.tsx`, `src/App.css`, `examples/characters/lyra/chats/session-welcome.md`, `README.md`, `docs/PROJECT.md`, `docs/PLAN.md`, `docs/DECISIONS.md`, `docs/STATUS.md`, `docs/Future-Growth-Notes.md`, and this handoff.

Decisions added/superseded: ADR-012 accepted.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` PASS 128 passed, 2 ignored. `cargo fmt --all -- --check` PASS. `cargo clippy --all-targets -- -D warnings` PASS. `npm run lint` PASS. `npm run typecheck` PASS. `npm run build` PASS. Native window interaction was not re-run.

Incomplete work / blockers: none for T28. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-12 — T29: edit last message, regenerate, and continue

Scope and outcome: last-exchange edit, regenerate, and continue. Transcript status `superseded` was added (schema still 1). Regenerating a complete reply keeps the previous assistant text in Markdown as superseded, mints a new assistant id, and extracts only the replacement. Continue appends to an Interrupted reply with the same turn id and extracts once when that turn becomes Complete. Edit last user updates that turn in place, supersedes following assistant turns, then generates a new assistant id. Retry remains for failed or empty replies and no longer truncates superseded siblings. Prompt history and chat bubbles omit superseded turns. Extraction jobs whose assistant source is no longer Complete are abandoned without writing memories. Memories already committed from a superseded reply are left in place.

Files changed: `src-tauri/src/storage.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/extraction.rs`, `src-tauri/src/prompt.rs`, `src-tauri/src/lib.rs`, `src/conversation.ts`, `src/ConversationPanel.tsx`, `src/App.css`, `README.md`, `docs/PROJECT.md`, `docs/PLAN.md`, `docs/DECISIONS.md`, `docs/STATUS.md`, `docs/Future-Growth-Notes.md`, and this handoff.

Decisions added/superseded: ADR-013 accepted.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` PASS 135 passed, 2 ignored. `cargo fmt --all -- --check` PASS. `cargo clippy --all-targets -- -D warnings` PASS. `npm run lint` PASS. `npm run typecheck` PASS. `npm run build` PASS. Native window interaction was not re-run.

Incomplete work / blockers: none for T29. Swipe carousel, editing older turns, continue of Complete/max-token replies, and deleting memories from superseded replies remain out of scope. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-13 — T30: per-character model and sampling

Scope and outcome: optional per-character chat model, temperature, and max tokens. Provider connection (kind, endpoint, token, embedding model) stays app-global; Settings → Provider chat model is the inherit default. Overrides live in `{vault}/.tz-chatter/generation.json` and are never written to `character.md`. Empty fields inherit the active provider. Chat send/retry/regenerate/continue/edit and initiative resolve the override in Rust at request time and send sampling on Ollama `options` (`temperature`, `num_predict`) and OpenAI-compatible `temperature`/`max_tokens`. Extraction uses the character’s resolved chat model with conservative sampling (temperature 0, max tokens 1024), not the character’s creative temperature or cap. Embeddings stay on the global embedding model. Packs omit `generation.json`. The Characters editor has Chat model / Temperature / Max tokens fields; the conversation header shows the effective model.

Files changed: `src-tauri/src/generation.rs`, `src-tauri/src/providers.rs`, `src-tauri/src/connections.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/extraction.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/portability.rs`, `src/generation.ts`, `src/CharacterPanel.tsx`, `src/ConversationPanel.tsx`, `src/ProviderPanel.tsx`, `src/activeSession.ts`, `README.md`, `docs/PROJECT.md`, `docs/PLAN.md`, `docs/DECISIONS.md`, `docs/STATUS.md`, `docs/Future-Growth-Notes.md`, and this handoff.

Decisions added/superseded: ADR-014 accepted.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` PASS 146 passed, 2 ignored. `cargo fmt --all -- --check` PASS. `cargo clippy --all-targets -- -D warnings` PASS. `npm run lint` PASS. `npm run typecheck` PASS. `npm run build` PASS. Native window interaction was not re-run.

Incomplete work / blockers: none for T30. Named generation presets and opt-in pack export of sampling remain unscheduled. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-13 — Future-Growth-Notes: mark T25 implemented

Scope and outcome: documentation only. Marked background embedding rebuild and hybrid-as-default as **IMPLEMENTED (T25)** in `docs/Future-Growth-Notes.md` and moved it out of the still-on-mission list. The note now matches ADR-009 / T25: hybrid default when an embedding model is set and the index is ready, lexical fallback plus background rebuild when stale, Settings opt-out. Remaining on that item: no dedicated freshness panel; live embedding quality unmeasured. Embedding-model discovery stays under first-run provider coach.

Files changed: `docs/Future-Growth-Notes.md` and this handoff.

Decisions added/superseded: none.

Verification and actual results: none. No application code.

Incomplete work / blockers: none for this note. Contradiction/supersession review, keyboard-first chat, and macOS/Linux verification remain on-mission.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-13 — Contradiction/supersession review spec drafted

Scope and outcome: documentation only. The user promoted the on-mission “these two memories disagree” list as the next feature. Approved product rules: resolve by excluding the stale Markdown file (not delete, not body mutation); contradicts sides are user-picked; a pair leaves the list when either side is excluded or deleted. Approach 1: Conflicts section on Memories over existing `memory_relationships` in durable `state.sqlite3`.

Draft spec: `docs/specs/2026-09-13-contradiction-supersession-review-design.md`. Commands: `memory_conflict_pairs` (read, join Markdown, hide related/missing/excluded) and `memory_exclude_conflict_side` (fingerprint-checked exclude of one still-open pair; supersedes may only exclude `to_id` on this path). No T31/ADR yet.

Files changed: `docs/specs/2026-09-13-contradiction-supersession-review-design.md`, `docs/STATUS.md`, and this handoff.

Decisions added/superseded: none until acceptance.

Verification and actual results: none. No application code.

Incomplete work / blockers: waiting on spec review. Keyboard-first chat and macOS/Linux verification remain unscheduled.

Next concrete action: user reviews the spec. On acceptance, add T31 and ADR-015, then implement.

## 2026-09-13 — T31: contradiction and supersession review

Scope and outcome: Memories now lists committed `contradicts` and `supersedes` pairs from durable `memory_relationships`. `memory_conflict_pairs` joins Markdown, hides `related`/missing/excluded sides, and does not write. `memory_exclude_conflict_side` fingerprint-checks and sets `review_status: excluded` on one still-open pair; supersedes may only exclude `to_id` on this path. Contradicts sides are user-picked. Both files remain; FTS drops the excluded body. Locked files can be excluded by the user. The Conflicts section sits after the proposal queue; clicking a side opens the existing editor.

Files changed: `src-tauri/src/conflicts.rs`, `src-tauri/src/extraction.rs`, `src-tauri/src/lib.rs`, `src/memory.ts`, `src/MemoryPanel.tsx`, `src/App.css`, `README.md`, `docs/specs/2026-09-13-contradiction-supersession-review-design.md`, `docs/PLAN.md`, `docs/PROJECT.md`, `docs/DECISIONS.md`, `docs/STATUS.md`, `docs/Future-Growth-Notes.md`, and this handoff.

Decisions added/superseded: ADR-015 accepted.

Verification and actual results: `cargo test --manifest-path src-tauri/Cargo.toml` PASS 156 passed, 2 ignored. `cargo fmt --all -- --check` PASS. `cargo clippy --all-targets -- -D warnings` PASS. `npm run lint` PASS. `npm run typecheck` PASS. `npm run build` PASS. Native window interaction was not re-run.

Incomplete work / blockers: none for T31. Chat-chrome badges, keep-both dismiss, and Markdown-stored links remain out of scope. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-13 — T32: keyboard-first chat

Scope and outcome: Chat now has a global keymap and a typeahead character switcher. Ctrl+N starts a new session, Ctrl+K opens the library picker, Ctrl+L focuses the composer, Ctrl+. stops generation, Ctrl+I toggles Sources when memories were retrieved. Escape closes switcher, then Settings, then stops generation, then Sources, then focuses the composer. Session rename still owns Escape. Cmd mirrors Ctrl on macOS without a macOS support claim. Choosing a character resumes that vault through the existing load path, stays on Chat, and focuses the composer. Mouse controls remain.

Files changed: `src/chatShortcuts.ts`, `src/CharacterSwitcher.tsx`, `src/App.tsx`, `src/ConversationPanel.tsx`, `src/SettingsPanel.tsx`, `src/activeSession.ts`, `src/App.css`, `tests/chatShortcuts.test.mjs`, `package.json`, `README.md`, `docs/PLAN.md`, `docs/PROJECT.md`, `docs/DECISIONS.md`, `docs/STATUS.md`, `docs/Future-Growth-Notes.md`, and this handoff.

Decisions added/superseded: ADR-016 accepted.

Verification and actual results: `npm run test:shortcuts` PASS 17 passed. `npm run lint` PASS. `npm run typecheck` PASS. `npm run build` PASS. Native window chord click-through was not re-run.

Incomplete work / blockers: none for T32. Command palette, session picker in the overlay, customizable bindings, and edit/regenerate/continue keys remain out of scope. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-13 — ADR-017: split macOS / Linux verification

Scope and outcome: split the combined “macOS / Linux verification” growth item. Linux is scheduled as T33 (desktop launch, Windows-copied vault round-trip, one chat turn, named packaging/tray results). macOS stays intended but unverified: Cmd mirrors Ctrl as a portable binding, no Darwin task, no Mac download or prerequisites. Windows remains the verified target. No application code, CI, or packaging changes.

Files changed: `docs/DECISIONS.md`, `docs/PROJECT.md`, `docs/PLAN.md`, `docs/STATUS.md`, `docs/Future-Growth-Notes.md`, `README.md`, and this handoff.

Decisions added/superseded: ADR-017 accepted. ADR-002 remains in force; it now points at ADR-017 for platform *claims*.

Verification and actual results: documentation-only. T33 appears in PLAN (Milestone 16) and STATUS (`todo`). No Linux or macOS runtime was run.

Incomplete work / blockers: T33 needs a Linux host with a display; do not start it from this Windows workspace. WSL2/WSLg is not a general Linux claim. macOS remains unscheduled until a Darwin host can launch the window. T16/T18 visual tray/keyboard and live llama.cpp/LM Studio checks remain environment-limited.

Next concrete action: on a Linux desktop, mark T33 `in_progress` and follow its PLAN acceptance list.

## 2026-09-13 — T33: Linux desktop verification

Scope and outcome: ran T33 on Ubuntu 24.04.5 LTS, GNOME on Wayland, in a VirtualBox guest whose project path is `vboxsf` (`/media/sf_tz-chatter`). Symlinks are denied on that share, and the tree still had Windows `node_modules` (win32 native bindings) plus a 23G `src-tauri/target`, so the run used a native ext4 worktree at `/home/tag/src/tz-chatter` with `CARGO_TARGET_DIR=/home/tag/.cache/tz-chatter/target`. User-space rustup 1.98.1 and Node 24.21.0; Tauri GTK `-dev` packages via sudo. Distro Node 18.19.1 is too old for Vite 8.

The desktop window opened (`npm run tauri dev`: tz-chatter + WebKit network/web processes, Wayland cursor fd). A copy of the Windows Lyra vault at `/home/tag/tz-chatter-vaults/lyra-from-windows` received one streamed chat turn through the live `conversation_send` IPC path against a local Ollama-shaped stub on `127.0.0.1:11434` (Ollama itself was not installed). `chats/session-welcome.md` contains the T33 user turn and stub assistant reply. After killing and relaunching the process, resume still reported Lyra, 9 turns, and the T33 pair. Tray icon registered with GNOME AppIndicator (`ubuntu-appindicators@ubuntu.com`, StatusNotifier item `tray_icon_tray_app_*`). Initiative notification was not visually clicked. Packaging produced deb and AppImage; the release binary launched and re-registered the tray.

One code fix from Clippy 1.98 (not present on Windows rustc 1.94): `decode_vector` now uses `as_chunks::<4>()` instead of `chunks_exact(4)`.

Files changed: `src-tauri/src/embeddings.rs`, `README.md`, `docs/STATUS.md`, `docs/PROJECT.md`, `docs/DECISIONS.md`, `docs/Future-Growth-Notes.md`, and this handoff.

Decisions added/superseded: ADR-017 remains accepted; its Linux paragraph now records the Ubuntu 24.04.5 GNOME/Wayland T33 run instead of scheduling it. It still forbids a general “Linux is supported” claim.

Verification and actual results:
- `npm run lint`, `typecheck`, `build`, `test:shortcuts` (17 passed)
- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`
- `cargo test --manifest-path src-tauri/Cargo.toml` — 156 passed, 2 ignored
- `cargo clippy --all-targets -- -D warnings`
- `npm run tauri dev` window + tray; Windows-copied vault turn; restart resume
- `npx tauri build --bundles deb,appimage` — `tz-chatter_0.1.0_amd64.deb` 5,294,836 bytes, `tz-chatter_0.1.0_amd64.AppImage` 81,373,688 bytes
- packaged `/home/tag/.cache/tz-chatter/target/release/tz-chatter` launched with tray

Incomplete work / blockers: live Ollama/llama.cpp/LM Studio not present on this guest. Initiative notification not visually exercised. WSL2/WSLg was not used and is still not a general Linux claim. macOS remains unscheduled. T16/T18 Windows visual leftovers remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-13 — T34: composer emoji picker

Scope and outcome: added an in-app emoji picker on the chat composer only. A labeled footer button opens a popover with search and Unicode categories (smileys, people, nature, food, activity, travel, objects, symbols). Clicking a glyph inserts it at the caret or replaces the selection; the picker stays open for more inserts and returns focus to the composer. Escape, a second button click, or a click outside closes it. Opening focuses search; Enter in search does not send. The button is disabled with the composer (no vault, or while editing the last user turn). No new global shortcut, npm emoji library, Rust commands, recents, or other textareas. Glyphs are system Unicode and round-trip in Markdown transcripts.

Escape order is now: switcher, Settings, rename (unchanged owner), emoji picker, new-local, stop generation, Sources, focus composer.

Files: `src/emojiCatalog.ts`, `src/EmojiPicker.tsx`, `src/ConversationPanel.tsx`, `src/chatShortcuts.ts`, `src/App.tsx`, `src/activeSession.ts`, `src/App.css`, `tests/emojiPicker.test.mjs`, `tests/chatShortcuts.test.mjs`, `package.json`, `README.md`, `docs/PLAN.md`, `docs/PROJECT.md`, `docs/STATUS.md`, and this handoff.

Decisions added/superseded: none. Static frontend catalog; no OS emoji panel.

Verification and actual results: `npm run test:unit` 25 passed; `npm run lint`; `npm run typecheck`; `npm run build`. Native window click-through was not re-run.

Incomplete work / blockers: none for T34. Recents and other textareas stay out of scope. T16/T18 visual/live-provider leftovers remain environment-limited.

Next concrete action: none scheduled. T16/T18 remain environment-limited.

## 2026-09-13 — R01: first-release functionality, UX, and reliability review

Scope: user requested a broad review of functionality, UI/UX, safe edge cases, and release readiness. Reviewed clean application revision `350a0f9272b1529a8180fcbb59e3bffd54980d41`. No application behavior was changed. Added `docs/RELEASE_REVIEW.md`, diagnostic/screenshot artifacts under `docs/review/`, R01 scope in PLAN, and review state in STATUS. No decisions/ADRs added. R01 is the completed review, not completion of remediation or the overall project.

Outcome: hold a public release for hardening. Fifteen prioritized findings cover parser panic, fabricated evidence accepted as user fact, missing extraction schema, stale replies after character switches, uncancellable HTTP header waits, external-edit loss, alternate memory-disclosure paths, malformed-memory chat failure, running-job resets on queue open, hidden accepted proposals, stale initiative session binding, draft erasure, IME submission, minimum-window library collapse, and modal focus. Additional source risks cover export consistency, confined paths, embedding-space identity, scene collisions, failure visibility, editor isolation, partial streams, and scale. Full triggers, source references, severity, evidence levels, and fix order are in the report.

Verification: 156 Rust tests passed (2 ignored); 25 frontend tests passed; fmt/strict Clippy/lint/typecheck/build passed. Both previously ignored live Ollama smoke tests passed against installed `llama3.2:latest` (native and OpenAI-compatible). Live extraction with the actual prompt, temperature 0 and max tokens 1024 returned JSON rejected for `origin: user`; it did not form a memory. Ten isolated Rust diagnostics reproduced observed defects, including a caught parser panic. Their PASS outcomes assert bad current behavior, not successful safety checks; they are stored outside the normal test suite. `docs/review/run-rust-diagnostics.ps1` was run and verified to remove its temporary Cargo integration test. No private vaults or real conversation data used.

UI: native computer-use and in-app browser Node runtimes failed at initialization. Shell execution required the permitted elevated execution path because the default sandbox helper failed before starting. Used existing Edge in an isolated headless profile with real React/Vite UI and synthetic Tauri IPC. At 1280x800/900x620, reproduced Beta displaying Alpha's late reply, next-draft deletion, IME Enter submission, Tab outside Settings, and a 30px-high character library with 200px content. Captures and machine-readable results are in `docs/review/`. This is not evidence of native IPC, tray, notifications, accessibility certification, or package install behavior.

Packaging: `npm run tauri build` passed for current source. MSI `src-tauri/target/release/bundle/msi/tz-chatter_0.1.0_x64_en-US.msi` is 4,644,864 bytes; NSIS `src-tauri/target/release/bundle/nsis/tz-chatter_0.1.0_x64-setup.exe` is 3,408,978 bytes. Authenticode status of current release executable: NotSigned. No installation/publishing performed; current packages are not release-approved. LM Studio at :1234 refused connection; llama.cpp was not tested; prior Linux evidence and unverified macOS scope unchanged. No dependency advisory/license audit or long-duration/large-vault soak was performed.

Next executable step: start remediation with F01-F03. Turn the diagnostic inputs into tests asserting graceful parser failure and correct evidence rejection; add a complete extraction schema and a real local-model memory-write/restart/recall test. Then fix generation identity guards/cancellation/conflicting writes and retarget initiative on every active-session change. Keep T16/T18 native interaction leftovers open. See the report for the remaining ordered gate; do not infer release readiness from baseline tests or rebuilt installers.

## 2026-09-13 - R02: release-review remediation

Scope and outcome: audited the implementation started from `docs/RELEASE_REVIEW.md`, reconciled all 15 findings and the listed additional risks, completed the remaining in-scope reliability work, and marked R02 done in `docs/STATUS.md`. The remediation protects extraction evidence and parsing, durable queue/review state, request identity and cancellation, transcript conflicts, remote-memory disclosure, Markdown isolation, initiative retargeting, drafts, IME, minimum-size layout, Settings focus, path/embedding identity, scene creation, exports, failure visibility, and partial streams.

Files changed in this continuation: `src-tauri/src/conversation.rs`, `src/ConversationPanel.tsx`, `src-tauri/src/connections.rs`, and the R02 status/review records. The working tree also contains the prior Grok remediation across the Rust/UI files listed in the earlier handoff; those changes were preserved.

Verification: `cargo test --locked --manifest-path src-tauri/Cargo.toml` PASS - 172 passed, 2 ignored; `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` PASS; `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` PASS; `npm run test:unit` PASS - 30; `npm run lint` PASS; `npm run typecheck` PASS; `npm run build` PASS; `git diff --check` PASS.

Important limits: the live extraction probe recorded in R01 rejected the model output, so no successful extraction -> restart -> recall journey is claimed. Native Windows install/tray/accessibility interaction, dependency/license audit, large-vault/soak testing, macOS, and additional live provider coverage remain release-gate work. R02 is implementation-complete, but public release remains on hold.

Next concrete action: use the ordered release gate in `docs/RELEASE_REVIEW.md` from a native Windows session, beginning with live automatic-memory -> restart -> recall and fresh package/install verification.

## 2026-09-13 — R03: GUI coherence remediation

Scope and outcome: documented the current GUI review as G01-G09 in `docs/GUI_REVIEW.md` and resolved every item. Character editing is now divided into Identity, Model, You, and Scenes tabs with independent save boundaries; generation values validate before writes. All character/vault/pack path workflows have Tauri native folder pickers plus editable fallbacks, with new export/restore destinations derived from a selected parent. Memories shows browse/edit first and compact zero-count Review/Conflicts afterward. Regenerate/Continue/Retry lives in the target assistant bubble; resume/error feedback is composer-adjacent. Initiative controls expose character/session prerequisites. Scene deletion uses a native warning confirmation. Emoji/switcher semantics are valid, active switcher options are exposed, and transient popovers do not stack.

Two additional defects found during implementation were fixed: New scene incorrectly required an id before its title-to-id step, making save unreachable; and an initial switcher-dismissal callback dispatched a React update during render. The rendered harness now proves New scene reaches `local_save` and records zero console/runtime errors.

Files changed: `docs/GUI_REVIEW.md`, `docs/PLAN.md`, `docs/STATUS.md`, `README.md`, this handoff, R03 render harness/results/screenshots under `docs/review/`, `package.json`/lock, `src-tauri/Cargo.toml`/lock, dialog capability/init, `src/FolderField.tsx`, `src/folderPath.ts`, `src/characterEditor.ts`, `src/initiativeAvailability.ts`, Character/Memory/Conversation/Settings/Initiative/Emoji/Switcher/App UI and CSS, and focused frontend tests.

Decisions added/superseded: none. R03 implements the reviewed UI without changing storage authority, provider boundaries, or product scope.

Verification and actual results: `npm run test:unit` PASS (42); lint/typecheck/build PASS; `cargo test --locked --manifest-path src-tauri/Cargo.toml` PASS (172 passed, 2 ignored); Rust fmt check PASS; strict Clippy PASS; `tauri build --no-bundle` PASS at `src-tauri/target/release/tz-chatter.exe`. `node docs/review/gui_remediation_review.mjs` PASS at 1280x800/900x620: response actions inside the assistant message, no invalid emoji listbox, switcher active descendant present, no stacked emoji, Character New/Add folder controls present, four editor tabs, New scene save invoked, Memory browse before queues, no list/editor/queue overlap, five Settings/Vault browse controls, and zero runtime errors.

Incomplete work / blockers: none for R03. Native computer-use still fails at host sandbox initialization, so native dialog click behavior, screen-reader/DPI checks, tray/notification interaction, install lifecycle, and the broader live-provider/release gate remain T16/T18 work.

Next concrete action: resume the ordered release gate in `docs/RELEASE_REVIEW.md`.

## 2026-09-14 — R04: portability folder-field alignment

## 2026-09-14 — T35: character-chat application icon

Scope and outcome: replaced the stock Tauri icon set with a generated project-specific character-chat mark. The source is src-tauri/app-icon-source.png; npx tauri icon regenerated the configured PNG, ICO, ICNS, Android, and iOS assets under src-tauri/icons/.

Two additional concepts were generated after feedback that the first mark still felt too Tauri-like. They are preserved but inactive at src-tauri/icon-alternatives/character-face.png and src-tauri/icon-alternatives/two-character-chat.png. No alternative was deleted or wired into the bundle.

Verification: npx tauri icon src-tauri/app-icon-source.png completed and the generated assets are present. Full application checks were not rerun because this task only changes image assets.

Follow-up selection: the user chose two-character-chat.png. It now exactly matches app-icon-source.png (SHA-256 00949BEE39427E82EEEE6371EA4E4E8B9079A5019E3234973319596825FAFA46), and npx tauri icon was rerun successfully to update every platform asset.

The selected PNG is also copied to src/assets/tz-chatter-icon.png and rendered by the sidebar brand in src/App.tsx with matching image sizing in src/App.css. The native taskbar icon continues to come from the regenerated Tauri bundle assets configured in src-tauri/tauri.conf.json.

Verification: npm run lint, npm run typecheck, npm run build, and git diff --check pass. Full native taskbar visual click-through remains part of the environment-limited T16/T18 release gate.

Native refresh follow-up: the prior debug executable was stale (its timestamp predated icon generation), which explained the screenshot still showing the Tauri mark. `npx tauri build --debug --no-bundle` rebuilt `src-tauri/target/debug/tz-chatter.exe` successfully, and that rebuilt executable was launched. Tauri’s configured `bundle.icon` entries already point at the regenerated `icon.ico`/PNG assets; no runtime `setIcon` workaround is needed.

Next concrete action: resume the ordered release gate in `docs/RELEASE_REVIEW.md`.

Scope and outcome: fixed the staggered Settings → Vault portability controls shown in the user-provided 1512×913 screenshot. The outer responsive grid gives all four folder fields the height of the longest help text; without an explicit content alignment, each nested grid stretched its label/control/help tracks differently. Adding `align-content: start` to `.folder-field` keeps every field's internal rows packed at the top, so labels and input/button rows align while help text remains directly below its own control.

Files changed: `src/App.css`, `docs/PLAN.md`, `docs/STATUS.md`, this handoff, and the existing GUI render harness/results/screenshots under `docs/review/`. The harness now measures the reported wide viewport before retaining its compact 900×620 capture; its preview URL uses `localhost`, matching Vite's default bind on this host.

Decisions added/superseded: none. R04 is a presentation correction and does not change product behavior, storage, provider, or portability boundaries.

Verification and actual results: `npm run test:unit` PASS (42); `npm run lint` PASS; `npm run typecheck` PASS; `npm run build` PASS. `node docs/review/gui_remediation_review.mjs` PASS with synthetic Tauri IPC: at 1512×913 all four portability labels report top `367.390625`, all four control rows report top `384.390625`, label/control spreads are both 0px; the 900×620 capture completed; runtime error count is zero. The temporary Vite preview process tree was stopped after verification.

Incomplete work / blockers: none for R04. Existing native Windows dialog/accessibility/tray and broader release-gate gaps remain T16/T18 work.

Next concrete action: resume the ordered release gate in `docs/RELEASE_REVIEW.md`.

## 2026-09-14 — T36: GitHub Linux CI and security scanning

Scope and outcome: added two Ubuntu 24.04 GitHub Actions workflows for the newly published `taggedzi/tz-chatter` repository. `ci.yml` runs the frontend unit suite, ESLint, TypeScript checks, frontend build, Rust formatting, locked Rust tests, strict Clippy, and npm/RustSec dependency audits on pushes and pull requests targeting `main`, plus manual dispatch. `codeql.yml` analyzes TypeScript and Rust on those events, manual dispatch, and a Monday weekly schedule. Both workflows use read-only repository permissions by default; only the CodeQL job receives `security-events: write` plus its required read permissions. README badges link directly to each workflow.

The initial RustSec audit found RUSTSEC-2026-0285 in transitive `rustls 0.23.44`, published on 2026-09-14. `src-tauri/Cargo.lock` now selects fixed `rustls 0.23.45`. The clean audit retains seven allowed warnings from upstream transitive crates (six unmaintained `proc-macro-error`/`unic-*` advisories and one `glib 0.18.5` unsoundness warning); cargo-audit's default vulnerability gate does not fail on those warnings.

Files changed: `.github/workflows/ci.yml`, `.github/workflows/codeql.yml`, `README.md`, `src-tauri/Cargo.lock`, `docs/PLAN.md`, `docs/STATUS.md`, and this handoff.

Decisions added/superseded: none. This is development/repository infrastructure and does not change runtime architecture.

Verification and actual results: YAML lint PASS; actionlint 1.7.12 PASS; `npm run test:unit` PASS (42); `npm run lint`, `npm run typecheck`, and `npm run build` PASS; Rust fmt PASS; `cargo test --locked --manifest-path src-tauri/Cargo.toml` PASS (172 passed, 2 ignored); strict Clippy PASS; `npm audit --audit-level=high` PASS with 0 vulnerabilities; `cargo audit` PASS with 0 vulnerabilities and 7 allowed warnings after the rustls update; `git diff --check` PASS.

Incomplete work / blockers: GitHub-hosted runs and live badge states cannot exist until the user commits and pushes these changes. CodeQL result upload can only be verified in the repository's Actions/Security UI.

Next concrete action: commit and push T36, inspect the first CI and CodeQL runs, and address any runner-only discrepancy. Then resume the ordered release gate in `docs/RELEASE_REVIEW.md`.

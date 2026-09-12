# Live project status

Last updated: 2026-09-12

## Current position

- Project phase: Milestone 7 in progress (T19 and T20 complete; T16/T18 remain open only for environment-limited checks).
- Implemented: project documentation, agent continuity system, local Git source control, Tauri desktop shell, provider connections, portable vault storage, conversation lifecycle with restart resume, bounded prompt construction, rebuildable lexical memory indexing, guarded lexical and hybrid memory retrieval, memory review UI, per-turn context inspection, automatic completed-turn extraction enqueue/worker validation, durable initiative eligibility, labeled initiative delivery with silence/stale-work handling, the first validated portable character-pack workflow, and T19 completion-audit remediations A01-A15.
- T01 delivered: React/TypeScript shell, Rust command boundary, frontend/Rust lockfiles, validation scripts, and a Windows launch check.
- Repository: initialized locally on `main`; no remote configured. T19/A13 commit `6a08a51` contains the reviewed application sources.
- Active task: none. T20 is done.
- Next task: retain T16/T18 in progress for visual Windows tray/keyboard interaction and live llama.cpp/LM Studio checks if those become available.
- Blockers: no known implementation blockers. Native Computer Use visual inspection remains unavailable; process/window inspection verified launch and responsiveness.

The user approved the architectural direction and requested a plan/status system that lets different agents continue without prior conversation context. T08 now retrieves bounded lexical memories into loopback-provider prompts; memory inspection, extraction, review, semantic ranking, initiative delivery, validated portable packs, and Windows packaging are implemented incrementally. Remaining release evidence is limited to visual Windows interaction coverage and live access to an OpenAI-compatible endpoint.

## Task board

The plan contains dependencies and acceptance criteria. This table is the authoritative state of each task.

| ID | Task | State | Evidence / remaining work |
| --- | --- | --- | --- |
| D00 | Project planning and continuity | done | Specification, plan, status, decisions, handoff, and AGENTS entry point created; documentation checks recorded below |
| D01 | Git source control | done | Initial commit `c0743bf`; ignore/whitespace checks passed; working tree verified clean after baseline commit |
| T01 | Desktop scaffold | done | Tauri 2 + React/TypeScript + Rust shell created; frontend/Rust checks and release build pass; Windows dev window launched and reported responsive |
| T02 | Provider contracts/configuration | done | Typed Rust/TypeScript provider contracts, capability reporting, validated/redacted settings, app-config persistence, and six contract tests |
| T03 | Character/transcript storage | done | Versioned character/memory/transcript/state formats, Lyra sample, vault path confinement, atomic writes, and 11 storage/provider tests |
| T04 | Provider connections | done | Ollama and OpenAI-compatible HTTP clients, discovery/chat/embedding parsing, cancellation-aware streaming, 16 deterministic tests, and one live Ollama smoke test |
| T05 | Conversation lifecycle | done | Rust lifecycle service persists complete/failed/interrupted replies, enforces idempotent user-turn keys and retry truncation, snapshots character/session identity, and exposes typed send/retry commands; the shell includes a local composer |
| T06 | Budgeted character prompts | done | Deterministic prompt builder reserves output capacity, orders character/scene/history/current context, trims oldest history, rejects oversized base input, and prevents duplicate current messages; 4 prompt tests plus integration coverage pass |
| T07 | Memory vault/lexical index | done | Markdown remains the source of truth; CRUD, rename/delete, fingerprint conflict detection, paragraph chunking, SQLite FTS5 sync/rebuild, exclusion, and character-isolated search are covered by tests |
| T08 | Retrieved memory in chat | done | Deduplicated lexical retrieval applies salience/recency/pinned budgets, source labels, prompt bounds, and character isolation; remote endpoints are guarded from vault disclosure by default |
| T09 | Memory UI/context inspector | done | Browse/search/edit/delete commands, explicit exclude/pin/lock controls, excluded-memory restoration path, and per-turn retrieved-source inspector verified |
| T10 | Extraction proposals/queue | done | Completed replies enqueue durable jobs; bounded provider worker builds labelled extraction prompts, validates source IDs, persists review-only proposals, and preserves bounded retry/restart behavior |
| T11 | Memory reconciliation/commit | done | Duplicate-job deduplication, deletion suppression, contradiction/supersession links, lock protection, Markdown-first commit, and index repair commands verified |
| T12 | Memory review/correction | done | Durable proposal queue UI with excerpts, edit/accept/reject/commit controls, persisted auto-commit preference, and distinct transcript/memory deletion semantics verified |
| T13 | Embeddings/versioned index | done | Explicit embedding-space metadata, fingerprinted canonical chunks, isolated SQLite vectors, similarity search, invalidation, interrupted rebuild recovery, and lexical fallback verified |
| T14 | Hybrid ranking/evaluation | done | Opt-in lexical/semantic ranking combines entity overlap, linked-memory, salience, recency, pinned priority, and type diversification; reasons and a deterministic evaluation fixture are covered; lexical remains the default pending provider scheduling |
| T15 | Initiative eligibility | done | Durable per-character settings and counters, quiet hours, inactivity, cooldown, frequency caps, unanswered/ignored backoff, disabled gates, and resume suppression are covered by controllable-clock tests and typed Settings controls |
| T16 | Initiative delivery/tray | in_progress | Core labeled delivery, explicit silence, bounded accepted open-topic selection, shared active-session cancellation, persisted user-activity cancellation, ignored/silence backoff, restart-resuming typed scheduler start/stop, native tray menu, separate notification settings, and native notification dispatch are implemented; full tray/notification interaction verification remains |
| T17 | Portability/recovery | done | Validated directory-based character packs, durable-state backup/restore, legacy manifest migration, traversal protection, no-overwrite restore, same-character backup preservation, and rebuildable-index recovery are implemented and tested |
| T18 | Windows validation/package | in_progress | Restart-resume journey, automatic extraction, provider selection/settings/health/discovery UI, live Ollama-native and Ollama OpenAI-compatible smoke tests, static UI checks, responsive launch, and MSI/NSIS package validation pass; visual Windows interaction and live llama.cpp/LM Studio evidence remain limited |
| T19 | Completion-audit remediation | done | A01-A15 resolved with tests/evidence in `docs/AUDIT.md`; 79 Rust tests passed and 2 ignored; strict Clippy, frontend lint/typecheck/build, MSI/NSIS rebuild, and two packaged launches passed. A11 live tray/keyboard Computer Use remains environment-limited. |
| T20 | Consolidate the chat layout | done | Chat is the default full-height transcript; sidebar lists sessions and starts new ones; Settings overlay holds provider, initiative, and vault/portability; Memories uses the loaded character. 80 Rust tests passed and 2 ignored; frontend lint/typecheck/build passed. |

## Active work and resumption

Task: T20 — consolidate the chat layout (complete)
Owner/session and date: Grok / current session — 2026-09-12
Scope / files being edited: `src/App.tsx`, `src/App.css`, `src/ConversationPanel.tsx`, `src/MemoryPanel.tsx`, `src/InitiativePanel.tsx`, `src/PortabilityPanel.tsx`, `src/ProviderPanel.tsx`, `src/SettingsPanel.tsx`, `src/activeSession.ts`, `src/conversation.ts`, `src-tauri/src/conversation.rs`, `src-tauri/src/storage.rs`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `docs/PLAN.md`, `docs/STATUS.md`, `docs/HANDOFF.md`, and `README.md`. No overlapping work reported.
Completed in this session: Confirmed prior T19 work was already committed (`8abe734`); `.vscode/` was left untracked. Rebuilt the desktop shell as a chat program: sidebar conversations, full-height transcript, composer, Settings overlay for provider/initiative/vault, and Memories bound to the loaded character. Added list/open/start session commands.
Remaining acceptance criteria: none for T20. T18 still lacks visual accessibility/keyboard interaction evidence and a live non-Ollama provider smoke test. T16 visual tray verification remains unavailable.
Verification commands and outcomes: `cargo test --manifest-path src-tauri/Cargo.toml` — PASS, 80 passed and 2 ignored; `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` — PASS; `npm run lint`, `npm run typecheck`, `npm run build` — PASS. Native window interaction was not re-run in this session.
Blocker and unblock action (if any): none.
Exact next step: retain T16/T18 in progress for remaining visual/tool and live non-Ollama provider limitations.

No background development jobs or running application servers remain.

## Verification ledger

| Date | Scope | Check | Result |
| --- | --- | --- | --- |
| 2026-09-11 | Initial workspace | Directory listing and file search | Empty workspace |
| 2026-09-11 | Initial repository | `git status --short --branch` | Not a Git repository; expected starting condition |
| 2026-09-11 | Documentation links | PowerShell: enumerate the seven Markdown documents, extract local Markdown links, and resolve with `Test-Path` | PASS: all six local links resolve |
| 2026-09-11 | Task consistency | PowerShell: extract plan/status IDs and compare uniqueness, membership, declared dependencies, and initial states | PASS: 19 matching unique IDs; dependencies exist and are acyclic in plan order; all 18 implementation tasks were `todo` at baseline |
| 2026-09-11 | T01 toolchains | Node.js/npm, Rust/Cargo, Tauri CLI inspection | PASS: Node v24.14.1, npm 11.19.1, rustc/cargo 1.94.1, Tauri CLI 2.11.4 |
| 2026-09-11 | T01 frontend | `npm run lint`; `npm run typecheck`; `npm run build` | PASS; Vite production bundle generated in `dist/` |
| 2026-09-11 | T01 Rust | `cargo check --manifest-path src-tauri\\Cargo.toml`; `cargo fmt --manifest-path src-tauri\\Cargo.toml --all -- --check` | PASS |
| 2026-09-11 | T01 Windows release | `npx tauri build --no-bundle` | PASS; built `src-tauri/target/release/tz-chatter.exe` |
| 2026-09-11 | T01 Windows runtime | `npm run tauri dev`; `Get-Process tz-chatter` | PASS; native window title `tz-chatter`, `Responding=True`; Computer Use visual inspection unavailable |
| 2026-09-11 | T02 provider contracts | `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 6 tests cover invalid config, unsupported capabilities, unavailable health, cancellation, redaction, and settings persistence |
| 2026-09-11 | T02 typed boundary | `npm run lint`; `npm run typecheck`; `npm run build`; `cargo check --manifest-path src-tauri\\Cargo.toml` | PASS |
| 2026-09-11 | T03 storage | `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 11 tests cover sample loading, vault round-trip, multiline transcript roles/statuses, metadata, path traversal, identity separation, and interrupted writes |
| 2026-09-11 | T03 cross-language checks | `npm run lint`; `npm run typecheck`; `npm run build`; `cargo check --manifest-path src-tauri\\Cargo.toml`; `cargo fmt --manifest-path src-tauri\\Cargo.toml --all -- --check`; `git diff --check` | PASS |
| 2026-09-11 | T04 protocol clients | `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 16 deterministic tests cover Ollama/OpenAI-compatible endpoints, payloads, fragmented streams, multibyte text, errors, embeddings, cancellation contracts, and provider isolation |
| 2026-09-11 | T04 live Ollama | `cargo test --manifest-path src-tauri\\Cargo.toml -- --ignored --nocapture` | PASS: existing Ollama at `127.0.0.1:11434`, discovered `llama3.2:latest`, and completed a real streamed chat reply in 18.84s |
| 2026-09-11 | T04 release/build | `npx tauri build --no-bundle`; frontend lint/type/build; Rust check/fmt | PASS; `src-tauri/target/release/tz-chatter.exe` built |
| 2026-09-11 | T05 conversation lifecycle | `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 20 passed, 1 ignored; tests cover complete/failed/interrupted replies, idempotent send, retry truncation, character/session isolation, and portable transcript persistence |
| 2026-09-11 | T05 cross-language boundary | `cargo check --manifest-path src-tauri\\Cargo.toml`; `cargo fmt --manifest-path src-tauri\\Cargo.toml --all -- --check`; `npm run lint`; `npm run typecheck`; `npm run build` | PASS; typed send/retry commands and the local conversation panel compile and bundle |
| 2026-09-11 | T05 desktop release | `npx tauri build --no-bundle` | PASS; built `src-tauri/target/release/tz-chatter.exe` after the conversation UI and command boundary changes |
| 2026-09-11 | T06 prompt construction | `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 24 passed, 1 ignored; prompt tests cover ordering, scene context, oversized base input, history trimming, conservative estimation, reserved output capacity, and current-message de-duplication |
| 2026-09-11 | T06 cross-language/release | `cargo check --manifest-path src-tauri\\Cargo.toml`; Rust fmt check; `npm run lint`; `npm run typecheck`; `npm run build`; `npx tauri build --no-bundle` | PASS; prompt-integrated desktop release built at `src-tauri/target/release/tz-chatter.exe` |
| 2026-09-11 | T07 memory index | `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 27 passed, 1 ignored; memory tests cover Markdown-backed create/edit/rename/delete, external-change conflict detection, paragraph chunking, excluded records, rebuild, and separate-vault isolation |
| 2026-09-11 | T07 cross-language/release | `cargo check --manifest-path src-tauri\\Cargo.toml`; Rust fmt check; `npm run lint`; `npm run typecheck`; `npm run build`; `npx tauri build --no-bundle` | PASS; bundled SQLite FTS5 release built at `src-tauri/target/release/tz-chatter.exe` |
| 2026-09-11 | T08 retrieval | `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 30 passed, 1 ignored; retrieval tests cover relevance, source labels, deduplication, salience/recency ordering, pinned budgets, token limits, prompt injection, and character isolation |
| 2026-09-11 | T08 cross-language/release | `cargo check --manifest-path src-tauri\\Cargo.toml`; Rust fmt check; `npm run lint`; `npm run typecheck`; `npm run build`; `npx tauri build --no-bundle` | PASS; guarded retrieval-integrated release built at `src-tauri/target/release/tz-chatter.exe` |
| 2026-09-11 | T09 memory review and context inspector | `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 31 passed, 1 ignored; added browsable excluded/locked-memory regression coverage |
| 2026-09-11 | T09 cross-language/release | `npm run lint`; `npm run typecheck`; `npm run build`; `npx tauri build --no-bundle` | PASS; memory review UI and context inspector release built at `src-tauri/target/release/tz-chatter.exe` |
| 2026-09-11 | T10 extraction queue | `cargo fmt --manifest-path src-tauri\\Cargo.toml --all -- --check`; `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 35 passed, 1 ignored; queue, parser, source verification, bounded retries, restart recovery, and review-only persistence covered |
| 2026-09-11 | T10 desktop release | `npm run lint`; `npm run typecheck`; `npm run build`; `npx tauri build --no-bundle` | PASS; extraction queue module included in `src-tauri/target/release/tz-chatter.exe` |
| 2026-09-11 | T11 reconciliation | `cargo fmt --manifest-path src-tauri\\Cargo.toml --all -- --check`; `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 40 passed, 1 ignored; duplicate jobs, suppression, lock protection, relationship links, newer edits, and index repair covered |
| 2026-09-11 | T11 cross-language/release | `npm run lint`; `npm run typecheck`; `npm run build`; `npx tauri build --no-bundle` | PASS; reconciliation commands included in `src-tauri/target/release/tz-chatter.exe` |
| 2026-09-11 | T12 proposal review | `cargo fmt --manifest-path src-tauri\\Cargo.toml --all -- --check`; `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 41 passed, 1 ignored; proposal edit/decision persistence and rejection suppression covered |
| 2026-09-11 | T12 cross-language/release | `npm run lint`; `npm run typecheck`; `npm run build`; `npx tauri build --no-bundle` | PASS; proposal review UI and commands included in `src-tauri/target/release/tz-chatter.exe` |
| 2026-09-11 | T13 embedding index | `cargo fmt --manifest-path src-tauri\\Cargo.toml --all -- --check`; `cargo test --manifest-path src-tauri\\Cargo.toml` | PASS: 45 passed, 1 ignored; space invalidation, fingerprinted chunks, vector search, isolation, rebuild recovery, and lexical fallback covered |
| 2026-09-11 | T13 cross-language/release | `npm run lint`; `npm run typecheck`; `npm run build`; `npx tauri build --no-bundle` | PASS; embedding index module included in `src-tauri/target/release/tz-chatter.exe` |

Application behavior now includes the T05 conversation lifecycle, T06 bounded prompt assembly, T07 Markdown-backed lexical indexing, T08 loopback-guarded memory retrieval, T09 memory review/context inspection, T10 completed-turn extraction enqueue and bounded worker validation, T11 reconciliation commands, T12 proposal review UI, T13 versioned vector indexing, T14 opt-in hybrid ranking with visible reasons, T15 durable initiative eligibility, T16 labeled delivery/tray/notification behavior, T17 validated portable packs, and T18 Windows packaging. Static UI semantics checks pass, but full tray/notification and visual accessibility interaction verification remains unavailable; OpenAI-compatible live support is not claimed without a reachable endpoint.

Git verification (2026-09-11): `git check-ignore -q` passed for nine ignored runtime/build paths and seven trackable source/lockfile/sample paths. `git diff --cached --check` passed. Initial commit `c0743bf` contains the nine intended files; `git status --short --branch` showed a clean `main`; `git remote -v` returned no remotes.

Local environment note: the initial sandbox-created `.git` directory is owned by the sandbox account. Git's exact-path `safe.directory` setting was added to the user's global configuration for `E:/Home/Documents/Programming/tz-chatter` so normal Git commands work. No wildcard trust or identity settings were changed. The sandbox filesystem helper intermittently failed, so Git setup and patching were completed through approved execution outside the sandbox.

## Known constraints and unresolved choices

- Windows is the initial verification target; other platforms are not yet tested.
- External local inference servers are the first integration mode. Engine installation, launching, and downloads are deferred.
- Exact dependency versions, packaging/signing details, default models, and quantitative performance targets remain to be selected during their tasks.
- Formal vault/transcript schema and migrations belong to T03; the specification establishes required properties, not a completed implementation.
- Provider, embedding, and tokenization behavior varies. Probe capabilities and record what was actually tested.

## Status update template

When work starts or pauses, replace the active-work section with:

```text
Task:
Owner/session and date:
Scope / files being edited:
Completed in this session:
Remaining acceptance criteria:
Verification commands and outcomes:
Blocker and unblock action (if any):
Exact next step:
```

Update the task board and append a handoff entry in the same change. Keep this file concise; move historical detail to `HANDOFF.md` while retaining the latest relevant verification and limitations here.
| 2026-09-11 | T14 hybrid retrieval | cargo test retrieval hybrid evaluation fixture | PASS: exact-name recall retained, semantic paraphrase recovered, stale and foreign-character candidates constrained; representative lexical latency 995 us versus hybrid 3166 us |
| 2026-09-12 | T15 initiative eligibility | cargo test initiative tests; frontend lint/typecheck/build; desktop release | PASS: 54 Rust tests passed and 1 ignored overall; durable settings/counters, quiet hours, caps, unanswered/ignored backoff, resume suppression, and persisted notification preference remain covered; release executable produced |
| 2026-09-12 | T16 initiative delivery | cargo test conversation/initiative/runtime tests; frontend lint/typecheck/build; desktop release; Windows dev launch | PASS: bounded accepted open-topic selection, labeled delivered turns, explicit SILENCE, shared cancellation/generation, no fabricated user turns, typed initiative_send, native tray and notification build, and responsive runtime launch verified; full tray/notification interaction remains |
| 2026-09-12 | T17 portability/recovery | cargo test portability; full Rust suite; frontend lint/typecheck/build; desktop release | PASS: pack round-trip preserves identity/memories/transcripts/operational state, legacy manifest migration works, traversal and unknown schemas are rejected, unrelated restore targets are protected, same-character replacement keeps a backup, corrupt indexes are rebuilt, 59 Rust tests passed and 1 ignored overall, and release executable produced |
| 2026-09-12 | T18 Windows validation/package | cargo test --ignored; npx tauri build; final packaged executable launch; frontend checks; static UI audit; performance checks | PASS: live Ollama smoke test passed in 27.59s; final MSI (4,378,624 bytes) and NSIS (3,219,523 bytes) packages produced; final packaged executable launched with Responding=True; lint, typecheck, build, and git diff checks passed; static audit found explicit button types and labels/ARIA for controls; representative retrieval measured lexical 1,089 us and hybrid 3,511 us, persisted-vault test process wall time 562.10 ms; OpenAI-compatible live support and visual accessibility/tray inspection remain unverified |
| 2026-09-12 | T18 restart-resume journey | focused resume test; full Rust suite; live Ollama; frontend lint/typecheck/build; README first-run docs | PASS: durable last-session tracking and character/session resume passed; full Rust suite 60 passed and 1 ignored; live Ollama smoke passed in 2.50s; frontend checks passed; subsequent final bundled rebuild is recorded below |
| 2026-09-12 | T18 final bundled release | `npx tauri build`; final executable launch; artifact inspection; `git diff --check` | PASS: restart-resume, automatic extraction worker, provider controls, and final keyboard/navigation UI included in final build; MSI 4,403,200 bytes and NSIS 3,237,648 bytes produced; executable launched with `MainWindowTitle=tz-chatter` and `Responding=True`; whitespace check passed |
| 2026-09-12 | T10/T18 automatic extraction | full Rust suite; focused enqueue/prompt tests; frontend checks; live Ollama; final package | PASS: completed assistant turns enqueue durable jobs, selected-provider worker validates source-labelled output, 61 Rust tests passed and 1 ignored, live Ollama passed in 2.50s, final MSI/NSIS built and executable remained responsive |
| 2026-09-12 | T18 provider controls | frontend lint/typecheck/build; final package; source audit | PASS: Conversation exposes persisted provider selection, optional bearer token, health check, model discovery, and OpenAI-compatible transport choice; final bundle includes the controls; live non-Ollama endpoint remains unavailable |
| 2026-09-12 | T16 scheduler connection | cargo fmt/check; full Rust suite; live Ollama smoke; frontend lint/typecheck/build; npx tauri build; packaged launch | PASS: cancellable 30-second scheduler start/stop commands share the active conversation session, persisted user activity/retry cancellation, explicit ignored/silence state updates, restart resume, provider-save refresh, and Initiative-panel controls compile and test; 62 Rust tests passed and 2 ignored, native Ollama passed in 2.56s and OpenAI-compatible Ollama passed in 2.49s; MSI 4,415,488 bytes and NSIS 3,248,865 bytes rebuilt; packaged executable responsive; visual tray verification remains |
| 2026-09-12 | T19 completion-audit remediation | cargo test; cargo fmt --check; clippy -D warnings; npm lint/typecheck/build; npx tauri build; two packaged launches | PASS: A01-A15 resolved with shipped-path tests; 79 passed and 2 ignored; strict Clippy passed; frontend checks passed; MSI 4,485,120 bytes and NSIS 3,294,811 bytes; exe launched twice with MainWindowTitle=tz-chatter Responding=True; A11 Computer Use interaction remains environment-limited |
| 2026-09-12 | T20 chat layout | cargo test; cargo fmt --check; npm lint/typecheck/build | PASS: 80 Rust tests passed and 2 ignored including session list/open/start; frontend checks passed; native window interaction not re-run |

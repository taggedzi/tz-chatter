# First-release review — 2026-09-13

**Recommendation: hold a public release and complete a focused reliability pass.** The feature set is substantial and the main chat layout is coherent, but current edge cases can crash the release build, lose drafts or external edits, misattribute replies, and undermine automatic memory formation. These are implementation gaps rather than a need for more features. A passing build and the existing tests do not establish that the full memory-backed chat journey works.

Reviewed application revision: `350a0f9272b1529a8180fcbb59e3bffd54980d41`. Starting working tree was clean. Application sources were not modified. Review artifacts use synthetic characters and temporary vaults; no user vault was used for diagnostics. Owner: Codex release-review. Live task state belongs in [STATUS.md](STATUS.md), row R01.

## Evidence and limits

| Check | Actual result |
| --- | --- |
| `npm run test:unit` | 25 passed |
| `npm run lint`, `npm run typecheck`, `npm run build` | Passed on current source; lint also passed after adding the review harness |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 156 passed, 2 live tests ignored in this run |
| Rust formatting and strict Clippy | Passed: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`; `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` |
| Existing live Ollama smoke tests | Both native and OpenAI-compatible health/discovery/stream tests passed using `llama3.2:latest` on port 11434 |
| Live automatic-extraction contract probe | **Rejected** output from `llama3.2:latest`, temperature 0, 1024 output tokens; required origin/schema fields were not supplied correctly |
| Focused Rust diagnostics | **10 defects reproduced**, with one additional live probe run separately. These diagnostics intentionally assert observed bad behavior; their passing is not evidence of safety |
| Real frontend in isolated headless Edge, synthetic Tauri IPC | Reproduced stale reply attribution, draft erasure, IME submission, missing modal focus containment, and a collapsed character library. Captures at 1280×800 and 900×620 |
| `npm run tauri build` | Passed; fresh MSI 4,644,864 bytes and NSIS installer 3,408,978 bytes under `src-tauri/target/release/bundle/` |
| Native interaction / installation | Not performed in this review. Native computer-use and in-app browser runtimes failed at initialization. The Edge harness exercises React with fake IPC, not the Rust desktop boundary, tray, notifications, or installer |
| Other platforms/providers | LM Studio port 1234 refused connection. No live llama.cpp test. Linux evidence is the prior named Ubuntu run; macOS remains unverified |

Environment: Windows workspace, Node v24.14.1, rustc 1.94.1. Packages remain local and are not approved release artifacts. No publishing, installation, or signing was performed. A process launch from earlier work does not substitute for current native interaction coverage. This review did not run a dependency advisory/license audit, a large-vault load test, or a long-duration soak test.

Reproductions: [Rust diagnostics](review/release_repros.rs), [PowerShell runner](review/run-rust-diagnostics.ps1), [frontend harness](review/ui_review.mjs), [frontend results](review/ui-results.json), [live extraction result](review/live-extraction-result.txt). The Rust runner stages a temporary Cargo integration test and removes it afterward; it refuses to overwrite an existing test file. The diagnostics are deliberately outside the normal test suite so expected-bug assertions do not become product regression requirements.

## Findings to fix before release

P1 means fix before distributing the application as reliable. P2 means a significant functional or usability defect to address during the hardening pass. Reproduced findings are distinguished from source inspection.

### F01 · P1 · Malformed extraction output can crash the application

**Reproduced.** `parse_response` finds the first opening brace and last closing brace, then slices without checking their order. Input `} malformed {` panics at [extraction.rs](../src-tauri/src/extraction.rs), line 784. The test catches this panic; the release profile sets `panic = "abort"`, so the same panic in a background task can terminate the process.

Fix: validate slice bounds and return a parse error for every malformed input. Add desired-behavior tests for reversed braces, incomplete JSON, arbitrary Unicode, fences, and surrounding text. Preserve bounded retries without a panic.

### F02 · P1 · Fabricated quotations pass the automatic user-fact gate

**Reproduced through classification and Markdown commit.** A source user turn said “I like quiet cafes.” A candidate cited that valid turn ID but supplied the invented quote “I live on Mars.” The application classified it as `user_stated` and wrote “The user lives on Mars.” to an accepted memory.

`effective_origin` checks only whether an evidence ID belongs to a user turn; it never verifies `evidence.quote` against the source content. `character_fact` and `conversation_event` pass through with no comparable evidentiary check. See [extraction.rs](../src-tauri/src/extraction.rs), lines 666–690 and 793–823.

Fix: require nonempty, matching source excerpts before automatic acceptance, validate completed source roles, and conservatively route unsupported claims to review. Quote matching is a necessary check, not proof that the candidate body follows from the quote; evaluate that remaining limitation explicitly.

### F03 · P1 · The extraction prompt omits the contract the parser requires

**Confirmed by a live local model.** `build_extraction_request` asks for a `proposals` array but does not describe required fields, allowed enum values, the `evidence` shape, or provide an example. The real `llama3.2:latest` probe returned `origin: "user"`, `quote`, and `speculation`; parsing rejected `user` as an unknown origin. No memory was formed by that probe. See [extraction.rs](../src-tauri/src/extraction.rs), lines 713–756.

The existing extraction tests supply correctly shaped output directly, so they cannot establish a working extraction integration. The worker also gives the model no existing memory IDs/content from which to propose meaningful contradiction/supersession links.

Fix: define the complete output schema in the request, use provider-supported structured output where available with a portable fallback, and add a live multi-turn test that actually produces Markdown, restarts, and recalls the fact. Exercise updated preferences and contradictions using known existing memory IDs. One failed model probe is not a claim that every model always fails; it demonstrates that the default supported path is not reliable enough yet.

### F04 · P1 · A late reply appears under the newly selected character

**Reproduced in the real frontend with delayed synthetic IPC.** Send to Alpha, switch to Beta, then complete Alpha's request. The UI shows `BETA · COMPLETE` above “Late reply from Alpha”; the sidebar can also revert to Alpha's session list. No chat-cancel command was issued during the switch. [Screenshot](review/switched-character-stale-reply.png).

`resumeVault` stops initiative but does not cancel chat. `runSnapshot`, `streamHandler`, and asynchronous session-list updates apply results without checking the active request/vault/session identity. See [ConversationPanel.tsx](../src/ConversationPanel.tsx), lines 168–213, 447–493, and 571–615.

Fix: attach a request generation/identity to every async result and stream callback; discard stale UI updates and cancel previous work on a switch. Cover rapid A→B→A switches, old errors, and list updates as well as successful completions. This reproduction proves UI misattribution; it does **not** prove the Rust reply was written into the wrong vault.

### F05 · P1 · Stop cannot cancel a provider that has not returned headers

**Reproduced with a loopback server that accepts the request but never sends HTTP headers.** Cancelling the token leaves the request pending. `ProviderClient` awaits `.send()` and error-body reads before entering the cancellation-aware stream loop, and constructs `Client::new()` without bounded request/connect behavior. See [connections.rs](../src-tauri/src/connections.rs), lines 20–24 and 102–145.

Because chat and extraction hold the shared model gate, stalled background requests can also prevent foreground chat from progressing. Health/discovery can wait indefinitely on a server that accepts connections but stalls.

Fix: race cancellation against the whole request, including header/error-body waits; bound connection and idle waits without imposing an unrealistically short total generation limit. Make model-gate acquisition cancellable. Test cancellation during cold model loading, queued work, headers, chunks, and stalled error responses.

### F06 · P1 · External edits can be overwritten despite the conflict checks

**Two paths reproduced.** (1) Open a memory in the editor, modify it externally, let retrieval or another browse refresh the index, then save the stale editor copy: the newer content is overwritten. `ensure_unchanged` compares disk to the shared index's current fingerprint, not to the version originally loaded by the editor. See [memory.rs](../src-tauri/src/memory.rs), line 351, and [lib.rs](../src-tauri/src/lib.rs), line 1560.

(2) Modify a transcript after generation starts but before it completes: the final save overwrites the edit with the earlier in-memory transcript. Reproduced with an external title change during transport. See [conversation.rs](../src-tauri/src/conversation.rs), lines 800 and 1000–1007, and [storage.rs](../src-tauri/src/storage.rs), line 364. Identity/persona/local save commands likewise lack editor-version conflict tokens.

Fix: pass the loaded fingerprint/version through the typed command boundary and compare it at commit time. Serialize writes per canonical vault/session and merge compatible metadata updates or return a recoverable conflict. Atomic replacement alone does not prevent lost updates; include two application instances in later verification.

### F07 · P1 · The non-loopback memory-disclosure guard has alternate paths around it

**Initiative path reproduced with an in-memory fake transport; no external request was sent.** With a non-loopback endpoint, `send_initiative` still loads accepted open-thread memories into `topic_context` before checking whether retrieved memories may be disclosed. That topic text enters the outgoing prompt. See [conversation.rs](../src-tauri/src/conversation.rs), lines 673–700, and [initiative.rs](../src-tauri/src/initiative.rs), line 119.

**Additional source-confirmed path:** after a successful extraction commit, `schedule_embedding`/`process_embedding_rebuild` can send all eligible memory chunks to the configured embedding endpoint without checking the loopback disclosure policy. See [lib.rs](../src-tauri/src/lib.rs), lines 1110, 1129, and 1278–1336. Chat's “Memory disclosure is disabled for non-loopback endpoints” message therefore does not describe all outbound memory paths.

Fix: enforce one explicit disclosure policy at every outbound memory boundary, including initiative topics and background embeddings; test with capturing transports before enabling remote memory use.

### F08 · P1 · Ordinary Markdown edits can disable both memory and chat

**Reproduced.** Memory text is stored twice, in YAML `body` and the Markdown body. Editing only the readable Markdown body causes `load_memory` to reject the record. Character prompts have the same duplicated representation. See [storage.rs](../src-tauri/src/storage.rs), lines 328–360.

One malformed memory makes `MemoryStore::sync/list` fail, which aborts chat after the user turn has already been persisted and before any assistant failure record is created. The reproduction leaves a lone user turn; the UI does not receive the updated transcript on that error path. See [memory.rs](../src-tauri/src/memory.rs), line 132, and [conversation.rs](../src-tauri/src/conversation.rs), lines 467–486 and 811–829. Ordinary editor changes and a disposable-index problem should not brick the conversation.

Fix: migrate to one authoritative body; isolate invalid files with filename-specific diagnostics; support a clearly reported degraded chat mode and durable failed/interrupted requests with a recovery action. Also test a corrupt lexical index and a crash between user-save and assistant-save.

### F09 · P2 · Opening the memory queue resets a job that is still running

**Reproduced using two queue handles.** `ExtractionQueue::open` unconditionally resets all running jobs to pending as “recovered after application restart.” Opening Memories while extraction is in flight invokes this constructor, so the live worker later rejects its result with “only a running job can accept output.” Attempts are decremented too, undermining retry bounds under repeated inspection. See [extraction.rs](../src-tauri/src/extraction.rs), lines 187–199.

Fix: perform recovery once under an application/worker lease, not on every connection open. Add a test for opening review/conflict views while the worker is awaiting a provider.

### F10 · P2 · Accepting without committing hides the proposal after refresh

**Reproduced.** With the default manual-commit preference, click Accept, then Refresh or revisit Memories. The accepted proposal disappears even though no Markdown memory exists. `memory_review_queue` uses `pending_proposals`, which selects only `needs_review`; the UI expects accepted items to remain available for “Commit accepted memory.” See [extraction.rs](../src-tauri/src/extraction.rs), line 429, [lib.rs](../src-tauri/src/lib.rs), line 1631, and [MemoryPanel.tsx](../src/MemoryPanel.tsx), line 126.

Fix: return both actionable states or combine acceptance/commit transactionally; preserve a retryable accepted state when storage fails. Test refresh and restart between the two actions.

### F11 · P2 · Initiative remains attached to an old session

**Source-confirmed.** `openSession` and `startSession` only load/apply transcripts; they do not stop and retarget the scheduler. The scheduler captures a session ID and evaluates with `active_character: true`. After opening another session, an initiative can be saved in the old session and filtered out by the UI; a later character switch only stops the currently recorded session's scheduler. A new unsaved vault also generates a session UUID separately in `resumeVault` and `applyResume`. See [ConversationPanel.tsx](../src/ConversationPanel.tsx), lines 140–245, and [lib.rs](../src-tauri/src/lib.rs), lines 331–384 and 405–494.

Fix: centralize active vault/session transitions in Rust, enforce a single eligible scheduler for that selection, and cancel old work on every transition. Verify New, Open, archive, switch, provider save, sleep/resume, disable, and quit end to end.

### F12 · P2 · Finishing a reply deletes a newly typed draft

**Reproduced in the frontend.** The composer remains editable during generation. Type the next message while waiting; completion executes `setDraft("")` and deletes it. See [ConversationPanel.tsx](../src/ConversationPanel.tsx), line 483. The captured result is `draftAfterCompletion: ""`.

Fix: clear the submitted draft when it is accepted, or clear only if the value still matches the submitted snapshot. Preserve drafts by character/session and on failures.

### F13 · P2 · Enter submits while the user is composing IME text

**Reproduced with an `isComposing: true` keyboard event against the actual composer.** The global shortcut resolver handles IME, but the composer Enter handler does not. Enter used to select a Japanese/Chinese/Korean composition candidate sends the message. See [ConversationPanel.tsx](../src/ConversationPanel.tsx), line 888.

Fix: ignore submit while composing and add a component-level event test; retain Shift+Enter behavior.

### F14 · P2 · Minimum-size layout collapses the character library

**Visually reproduced and measured.** At the configured 900×620 minimum, the single-column character layout leaves its library only **30 pixels high** for 200 pixels of content. The heading, New/Add controls, and character entries are hidden behind an almost unusable internal scrollbar. [Screenshot](review/characters-900.png). See [App.css](../src/App.css), lines 733–736 and 812–815.

Fix: give stacked sections natural/content-sized rows and avoid shrinking the library to make room for the long editor. Recheck 900×620, 1280×800, display scaling, and large text with populated libraries and memory queues.

### F15 · P2 · Settings is announced as modal but keyboard focus escapes it

**Browser reproduction plus source inspection.** Opening Settings leaves focus outside the dialog, and Tab moves through controls behind the overlay. There is no focus trap, inert background, or focus restoration despite `aria-modal="true"`. See [SettingsPanel.tsx](../src/SettingsPanel.tsx), line 27, and [App.tsx](../src/App.tsx), line 161. Global chat shortcuts also remain active while editing Settings.

Fix: implement modal focus entry/containment/restoration and define which global shortcuts may run inside forms. Verify with native keyboard and screen-reader interaction; the browser result alone is not an accessibility certification.

## Additional reliability work found by source inspection

These are actionable risks or verification gaps, not claims that every listed failure has been reproduced.

- **Backup consistency:** `export_pack` copies canonical files and the live `state.sqlite3` independently with `fs::copy` ([portability.rs](../src-tauri/src/portability.rs), line 121). There is no vault-wide snapshot or coordination with workers. Review state can refer to a memory created after the export file list was taken, and a raw SQLite copy is unsafe during writes or when WAL holds committed state. Use a database snapshot mechanism plus coordinated canonical-file versions; test export during extraction and import over an active vault.
- **Path confinement is incomplete:** `generation_path` directly joins `.tz-chatter/generation.json`; extraction opens `.tz-chatter/state.sqlite3` directly. These bypass `Vault::resolve_relative`, so a junction/symlink on `.tz-chatter` can redirect writes outside an added vault. See [generation.rs](../src-tauri/src/generation.rs), line 70, and [extraction.rs](../src-tauri/src/extraction.rs), line 142. Test all durable paths with synthetic link fixtures and use the same confinement primitive everywhere. Pack traversal tests do not cover arbitrary existing vaults.
- **Embedding identity misses endpoint/revision:** vectors are keyed by provider ID, model name, dimensions, and index/chunk versions, not endpoint or model digest/revision ([embeddings.rs](../src-tauri/src/embeddings.rs), line 15; [conversation.rs](../src-tauri/src/conversation.rs), line 1185). Changing the endpoint under the same provider ID/model string can reuse incompatible vectors. Include stable provider/model identity in invalidation and test endpoint switches with equal dimensions.
- **New local can overwrite an existing scene:** `createSessionLocal` derives an ID from the title and calls the same unconditional `save_local` used for editing ([ConversationPanel.tsx](../src/ConversationPanel.tsx), line 651; [scene.rs](../src-tauri/src/scene.rs), line 181). A repeated/sluggified title can replace a shared scene used by other sessions. Separate create-if-absent from conflict-checked update.
- **Failure explanations are lost:** stream failures become a persisted status but their error message is discarded by `run_reply`; the frontend ignores `failed` stream events. A missing model can present an empty “failed” bubble without the actionable provider explanation. Sources/budget/fallback details are rendered only if at least one memory was retrieved, hiding precisely the no-memory/disabled/fallback cases. See [conversation.rs](../src-tauri/src/conversation.rs), line 970, and [ConversationPanel.tsx](../src/ConversationPanel.tsx), lines 581–589 and 804. Expose the actual failure and always make the context report reachable.
- **Memory/editor state across switches:** Memories updates its root/character but retains selections and pending UI state, and its refresh has no request-generation check. Treat all editor/list requests as character-scoped; reset or preserve drafts explicitly rather than letting a previous character's data linger.
- **Partial streams and output limits:** decoder errors propagate with `?`, dropping accumulated events; completion reasons such as token limits are treated as Complete. Review partial-output preservation, retry vs continue semantics, and the reduced-context retry's unchanged `request.max_tokens`. Add transport-level failure injection, not just decoder happy-path tests.
- **Scalability is unestablished:** session search reads full Markdown transcripts; memory retrieval repeatedly lists/syncs the vault; synchronous IPC handlers can perform substantial file work. Current small fixtures do not establish interactive performance with years of chats, thousands of memories, or large portraits. Measure representative sizes before stating practical limits.

## UI/UX assessment

The chat-first layout, visible active character, session search/archive, fixed composer, and separate Settings/Memories views are a sensible foundation. The warm transcript area and darker navigation make the hierarchy clear. The 1280×800 chat and settings captures did not show gross overlap. The problems above are more consequential than a visual redesign.

After correctness fixes, prioritize:

1. **First run:** provide folder pickers and a direct sample-character action. Current text-path fields and references to `character.md` assume knowledge of the storage layout. The source sample is not included by a bundle `resources` entry, so do not assume installed users have the README's repository-relative example path.
2. **Memory status:** show extraction pending/failed/last-saved state near chat, with a retry or explanation. The current empty review queue can mean “nothing to remember,” “still running,” “accepted but hidden,” or “all attempts failed.” Those states must be distinguishable before claiming automatic memory works.
3. **Readable controls:** the review/conflict count runs against its uppercase label, several operational labels/actions use 9–10 px text, and the Provider hybrid checkbox is spread awkwardly across its grid cell. Improve spacing, hit targets, and scaling before a general audience test. [Settings capture](review/settings-900.png), [Memories capture](review/memories-900.png).
4. **Long conversations:** avoid unconditional scroll-to-bottom whenever streamed turns update if the user is reading older content; add an explicit return-to-latest action. Consider safe Markdown rendering for code/lists/emphasis; current message rendering is plain text, which is safe but limits readability.
5. **Unsaved work and feedback:** preserve new drafts, show save/busy/error states consistently, and make session operations unavailable or visibly queued when they cannot currently run. Do not let enabled buttons silently ignore actions during generation.

## Remediation status - 2026-09-13

R02 completed the in-scope application hardening pass. The original findings now have the following status:

| Items | Status | Evidence |
| --- | --- | --- |
| F01-F03 | Implemented | Panic-safe parsing, evidence/role/quote validation, conservative origin reclassification, a complete portable extraction schema, and provider-native JSON mode with a portable prompt fallback; covered by extraction/provider tests. A successful live memory journey remains unverified. |
| F04-F05 | Implemented | Request identity/cancellation guards, cancellation through headers and error bodies, connect/read timeouts, and stale busy-state protection; frontend and provider tests pass. |
| F06 | Implemented | Transcript fingerprints cross the typed boundary; compatible metadata-only edits merge, body/turn conflicts become recoverable failures, failed replies append to the latest transcript, and scene saves are fingerprint-checked. |
| F07-F08 | Implemented | Remote memory disclosure is blocked for chat, initiative, and embedding rebuild; Markdown body is authoritative, invalid files are isolated, and failed/interrupted replies retain actionable text. |
| F09-F11 | Implemented | Queue recovery is worker-owned, accepted proposals remain in review, active-session transitions stop and retarget initiative, and scheduler replacement is cancellable. |
| F12-F15 | Implemented in source/tests | Drafts are keyed by vault/character/session and only matching submitted text is cleared; IME submission is guarded; minimum-window layout and Settings focus containment are addressed. Native keyboard, screen-reader, tray, and installer verification remain open. |
| Additional storage/privacy risks | Implemented where specified | SQLite export uses VACUUM INTO, durable paths use vault confinement, embedding identity includes endpoint, scene creation is no-overwrite, memory state is character-scoped, and partial/token-limited streams preserve output as interrupted. |
| Scalability and live release evidence | Open | No large-vault/soak benchmark, dependency/license audit, successful live extraction -> restart -> recall journey, fresh native Windows install test, or macOS/provider coverage was added by R02. |

The code and deterministic tests therefore satisfy the R02 implementation scope, but this does not convert the release recommendation into an approval. The final native and live-provider gate below remains required before public distribution.

## Release gate and suggested order

1. **Stop crashes and ungrounded memories:** F01–F03, F09–F10; add desired-behavior regression tests and a live automatic-memory→restart→recall journey.
2. **Protect user work and identity:** F04–F08 and F11–F13; cancellation, stale-response guards, recoverable failures, loaded-version conflict checks, consistent privacy policy, and one active scheduler.
3. **Close usability and storage gaps:** F14–F15, snapshot exports, confined durable paths, scene create collisions, visible extraction/provider errors, and embedding invalidation.
4. **Verify the shipped experience:** fresh native Windows profile, actual package install/upgrade/uninstall, character creation, streaming/stop, memory correction/retrieval, restart, export/import, tray show/hide/quit and notifications, keyboard/accessibility and high-DPI checks. Keep provider/platform claims limited to evidence. Use scratch vaults for destructive scenarios and test backup recovery explicitly.

The rebuilt MSI and NSIS packages prove current packaging works; they do not clear this gate. Before distribution, replace template package metadata (`A Tauri App`, `you`), choose distribution/license and signing arrangements, provide user-facing release notes and known limitations, and run dependency/license checks. No new feature work is necessary to make this release useful; reliable completion of the existing promises is the priority.

# Self-maintaining hybrid memory implementation plan

> **For agentic workers:** Follow `docs/PLAN.md` tasks T24 then T25. Use this file for files, signatures, tests, and sequence. Do not implement chat-chrome inbox badges or Remember-this.

**Goal:** After a completed chat turn, validated facts are written to Markdown automatically, and the next turn retrieves those files with hybrid search by default.

**Architecture:** Keep Markdown canonical. The extraction worker already proposes; T24 classifies origin from the transcript and auto-commits writable proposals through `ReconciliationService` (create new files only). T25 turns hybrid on when an embedding model is set, fails open to lexical if vectors are stale or unavailable, and rebuilds embeddings in a background job that yields to chat.

**Tech stack:** Existing Tauri 2 / Rust core / React UI. No new crates. No external vector database.

**Spec:** `docs/specs/2026-09-12-self-maintaining-hybrid-memory-design.md`

## Global constraints

- Users are not required to review, accept, or edit memories for the feature to work. Chat is the only required action.
- The model proposes; application code writes. Never let model output write Markdown directly.
- Workers create new memory files. They do not mutate an existing memory body. User edits (app or external) win via fingerprint. Locked files are never overwritten.
- Inferred guesses stay `needs_review` and are not indexed or retrieved until the user accepts them in Memories.
- `character.md` is never written by extraction.
- Hybrid is the default only when `embedding_model` is non-empty. Chat must succeed with visible lexical fallback otherwise.
- Loopback endpoints receive vault retrieval; other endpoints still get none.
- Chat, retry, character switch, and shutdown preempt extraction and embedding.
- Initiative turns are not extracted as facts.
- Tests first. Verify with `cargo test --manifest-path src-tauri/Cargo.toml`, then `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `npm run lint`, `npm run typecheck`.
- Update `docs/STATUS.md` and append `docs/HANDOFF.md` when a task starts and when it finishes. Do not mark T24/T25 done without the tests below.

## File map

| File | Responsibility |
| --- | --- |
| `src-tauri/src/extraction.rs` | `effective_origin`, persist reclassified origin, `superseded_targets` |
| `src-tauri/src/reconciliation.rs` | Auto-commit eligible proposals; Locked → leave inbox; create-only writes |
| `src-tauri/src/lib.rs` | After `accept_model_output`, auto-commit; background embed worker |
| `src-tauri/src/retrieval.rs` | Skip superseded target IDs |
| `src-tauri/src/conversation.rs` | Hybrid when requested and embedding model set; no sync rebuild; pass skip IDs |
| `src-tauri/src/embeddings.rs` | Used by background worker via existing `needs_rebuild` / `rebuild_precomputed` / `upsert` |
| `src/ProviderPanel.tsx` | Hybrid default on (`localStorage !== "false"`); optional lexical-only override |
| `src/ConversationPanel.tsx` | Same default when sending `use_hybrid_retrieval` |
| `src/MemoryPanel.tsx` | Leave inbox UI; auto-commit checkbox remains for *manual* Accept only |
| `README.md` | First-run: chatting accumulates memories without a review step |

---

## T24 — Auto-write validated memories

**PLAN.md ID:** T24. Depends on T12.

**Files:**
- Modify: `src-tauri/src/extraction.rs`
- Modify: `src-tauri/src/reconciliation.rs`
- Modify: `src-tauri/src/lib.rs` (`process_extraction_queue`, extraction worker tests)
- Test: modules already in those files (`#[cfg(test)]`)

**Interfaces:**

```rust
pub fn effective_origin(
    candidate: &ExtractionCandidate,
    transcript: &TranscriptDocument,
) -> ProposalOrigin

pub fn is_auto_writable(origin: &ProposalOrigin) -> bool

impl ExtractionQueue {
    pub fn reclassify_proposal(
        &self,
        proposal_id: &str,
        origin: ProposalOrigin,
    ) -> Result<MemoryProposal, ExtractionError>;

    pub fn superseded_targets(&self) -> Result<Vec<String>, ExtractionError>;
}

impl ReconciliationService<'_> {
    pub fn auto_commit(
        &mut self,
        proposal_id: &str,
    ) -> Result<CommitResult, ReconciliationError>;
}
```

`effective_origin` rules (verbatim from the spec):

- `inferred` → `Inferred`
- `user_stated` → `UserStated` only if at least one `evidence.turn_id` is a transcript turn with `TurnRole::User`; otherwise `Inferred`
- `conversation_event` / `character_fact` stay as labeled when parse already verified source-turn IDs exist; do not auto-write if origin was reclassified to `Inferred`

`is_auto_writable` is true only for `UserStated`, `ConversationEvent`, `CharacterFact`.

`auto_commit` marks the proposal accepted, calls the existing `commit_accepted` (which **creates** a new `MemoryRecord` via `MemoryStore::create` and never edits an existing body). On `CommitResult::Locked`, set the proposal back to `NeedsReview` so it remains inbox-only.

`process_extraction_queue` after a successful `accept_model_output(&job.id, &output)`:

1. For each returned proposal, compute `effective_origin` against the already-loaded transcript.
2. If origin changed, `reclassify_proposal`.
3. If not auto-writable, leave `needs_review`.
4. Else `auto_commit`.
5. Yield to chat (`tokio::task::yield_now`) as today.

Do not auto-commit if cancellation is set. Incomplete/failed extractor output stays on the existing fail/retry path.

### Steps

- [ ] **Write failing tests in `extraction.rs`**

```rust
#[test]
fn user_stated_without_user_turn_quote_is_inferred() {
    let transcript = /* user turn user-1, assistant turn asst-1 */;
    let mut candidate = /* origin UserStated, evidence turn_id asst-1 */;
    assert_eq!(effective_origin(&candidate, &transcript), ProposalOrigin::Inferred);
    candidate.evidence[0].turn_id = "user-1".into();
    assert_eq!(effective_origin(&candidate, &transcript), ProposalOrigin::UserStated);
}

#[test]
fn inferred_origin_never_becomes_auto_writable() {
    assert!(!is_auto_writable(&ProposalOrigin::Inferred));
    assert!(is_auto_writable(&ProposalOrigin::UserStated));
}
```

- [ ] **Run** `cargo test --manifest-path src-tauri/Cargo.toml effective_origin -- --nocapture`  
  Expected: FAIL (functions missing).

- [ ] **Implement `effective_origin` / `is_auto_writable` / `reclassify_proposal`.** `reclassify_proposal` updates `candidate_json` origin only; does not change status.

- [ ] **Write failing tests in `reconciliation.rs` and `lib.rs`**

Names and assertions:

- `auto_commits_user_stated_to_new_markdown` — proposal with user-turn evidence → `CommitResult::Committed`; `memories/semantic/*.md` exists; `pending_proposals` does not include it; FTS5 `search` hits the body.
- `inferred_stays_in_inbox_and_is_not_a_file` — origin inferred → still `needs_review`; `list()` does not contain the body.
- `assistant_labeled_user_stated_is_inferred` — model origin `user_stated` but evidence is assistant turn → inbox, no file.
- `duplicate_does_not_write_second_file` — second identical auto-commit → `Duplicate`, one `list()` match.
- `supersession_creates_new_file_and_records_link` — related unlocked memory + `Supersedes` → new id, `relationships_for(new)` contains the old id, old file body unchanged on disk.
- `locked_related_memory_goes_to_inbox` — existing test pattern; auto_commit returns `Locked`; proposal status `needs_review`; locked file bytes unchanged.
- `deleted_memory_is_not_resurrected` — suppress then auto_commit same source turns/type → `Suppressed`, no new file.
- In `lib.rs`, extend or add beside `extraction_worker_drains_pending_jobs_after_resume`: after the worker runs with `extraction_output("user-1")`, `MemoryStore::list` contains “User likes quiet cafes.” and `pending_proposals` is empty. Add a second job whose fake output uses `"origin":"inferred"` and assert that body is not in `list()` but is in `pending_proposals`.

- [ ] **Run those tests** — Expected: FAIL.

- [ ] **Implement auto-commit wiring in `process_extraction_queue`.** Keep `MemoryStore::create` as the only worker write. Do not add an in-place update path.

- [ ] **Run** `cargo test --manifest-path src-tauri/Cargo.toml`  
  Expected: all previously passing tests still pass, new tests pass.

- [ ] **Update STATUS/HANDOFF.** Mark T24 done only with the evidence above.

---

## T25 — Hybrid retrieval by default

**PLAN.md ID:** T25. Depends on T14, T24.

**Files:**
- Modify: `src-tauri/src/extraction.rs` (`superseded_targets`)
- Modify: `src-tauri/src/retrieval.rs`
- Modify: `src-tauri/src/conversation.rs` (`run_reply`, `build_hybrid_retrieval`)
- Modify: `src-tauri/src/lib.rs` (schedule/process embedding, like extraction)
- Modify: `src/ProviderPanel.tsx`, `src/ConversationPanel.tsx`
- Modify: `README.md` first-run step 4

**Interfaces:**

```rust
impl ExtractionQueue {
    pub fn superseded_targets(&self) -> Result<Vec<String>, ExtractionError>;
    // SELECT DISTINCT to_memory_id FROM memory_relationships WHERE relation = 'supersedes'
}

pub fn retrieve_with_report(
    store: &mut MemoryStore,
    query: &str,
    budget: RetrievalBudget,
    skip_memory_ids: &[String],
) -> Result<RetrievalReport, MemoryError>;

pub fn hybrid_retrieve(
    store: &mut MemoryStore,
    query: &str,
    semantic_candidates: &[SemanticSearchResult],
    related_memory_ids: &[String],
    skip_memory_ids: &[String],
    budget: RetrievalBudget,
) -> Result<Vec<RetrievedMemory>, MemoryError>;
```

Update every current caller of `retrieve` / `retrieve_with_report` / `hybrid_retrieve` to pass skip IDs (empty slice in tests that do not care).

In `run_reply`, when the endpoint is loopback:

```rust
let skip = ExtractionQueue::open(vault, &snapshot.character.id)
    .ok()
    .and_then(|queue| queue.superseded_targets().ok())
    .unwrap_or_default();
let prefer_hybrid = snapshot.use_hybrid_retrieval
    && snapshot
        .provider
        .embedding_model
        .as_deref()
        .is_some_and(|model| !model.trim().is_empty());
```

If `prefer_hybrid` is false because no embedding model: `retrieval_mode = "lexical"`, `fallback_reason = Some("Hybrid retrieval requested without an embedding model; lexical fallback used.")` only when `use_hybrid_retrieval` is true; if the user forced lexical (`use_hybrid_retrieval == false`), leave `fallback_reason` none and mode lexical.

**Do not embed on the chat path.** In `build_hybrid_retrieval`, if `index.needs_rebuild(&chunks)?` is true, return `Err("Embedding index is rebuilding; lexical fallback used.")` immediately. The conversation command then schedules background embedding (below) and uses the lexical report.

Background worker in `lib.rs`, mirror `schedule_extraction`:

- Key: `embedding:{character.id}`
- Take `model_gate` so it cannot preempt chat
- Open vault, `chunks_from_memories`, `EmbeddingIndex::open` with the snapshot’s embedding space
- If `needs_rebuild`, embed chunks via `ProviderClient::embed`, `rebuild_precomputed`
- Honor cancellation; on cancel, leave the index not-ready so the next chat still lexical-falls-back
- Yield with `tokio::task::yield_now`

Schedule it from the conversation send/retry path when hybrid was requested, an embedding model is set, and `needs_rebuild` was true or fallback_reason contains that rebuild sentence. Also schedule after T24 auto-commit if an embedding model is on the same provider snapshot used for extraction (pass `provider` already available in `schedule_extraction`). Prefer scheduling from `process_extraction_queue` after a successful auto-commit **and** from lexical-fallback-on-stale-index in `run_reply`, using `begin_background` so only one embed job runs per character.

UI:

- `ProviderPanel.tsx` and `ConversationPanel.tsx`: initialize hybrid with `localStorage.getItem("tz-chatter.use-hybrid-retrieval") !== "false"` so unset means on.
- Keep the checkbox as an opt-out: label it “Use hybrid semantic retrieval” still, default checked when an embedding model is set.
- Sending still passes `use_hybrid_retrieval` on the snapshot.

README first-run: after send, completed replies queue extraction; supported facts are saved as Markdown memories without an Accept click; open Memories only to fix mistakes or review guesses.

### Steps

- [ ] **Write failing retrieval test:** two memories, relationship `new supersedes old`, query matches both bodies; `hybrid_retrieve` / `retrieve_with_report` with `skip = superseded_targets()` returns only the new id.

- [ ] **Write failing conversation tests:**
  - `hybrid_used_when_embedding_model_and_flag_set` — existing hybrid fixture path, `use_hybrid_retrieval: true`, embedding model set, index ready → `retrieval_mode == "hybrid"`, `fallback_reason` none.
  - `lexical_fallback_when_index_needs_rebuild_does_not_embed` — index not ready; fake embed transport must not be called during `send`; `retrieval_mode == "lexical"`; fallback_reason mentions rebuilding.
  - `lexical_when_no_embedding_model` — `use_hybrid_retrieval: true`, `embedding_model: None` → lexical + existing hybrid-without-model reason.
  - Existing T14 evaluation fixture still passes under hybrid with skip IDs empty.

- [ ] **Run** those tests — Expected: FAIL (signature/skip/rebuild behavior missing).

- [ ] **Implement skip IDs, hybrid default gating, and no-sync-rebuild.** Add the background embed worker. Extend `extraction_worker` test only if auto-commit + embed schedule is testable without a live provider (use a fake embed client if one exists; otherwise unit-test `needs_rebuild` → schedule key uniqueness like `extraction:lyra` / `embedding:lyra` in `RuntimeState` tests).

- [ ] **UI default + README.** Frontend lint/typecheck.

- [ ] **Run** `cargo test --manifest-path src-tauri/Cargo.toml`  
  `cargo fmt --all -- --check`  
  `cargo clippy --all-targets -- -D warnings`  
  `npm run lint` / `npm run typecheck`  
  Expected: pass. Do not claim live embedding quality without a reachable embedding model; record that limitation in STATUS if the environment has none.

- [ ] **Update STATUS/HANDOFF.** Mark T25 done only with the tests above.

## Spec coverage

| Spec requirement | Task |
| --- | --- |
| Auto-write user_stated / event / character_fact | T24 |
| Inferred inbox-only, not retrieved | T24 + T25 skip of uncommitted (never a file, so FTS5 already excludes) |
| Origin reclassify: user_stated needs user-turn quote | T24 |
| Create new files only; locked/newer edit/inbox | T24 |
| Duplicate / supersede / deletion suppression | T24 |
| Hybrid default when embedding model set | T25 |
| Lexical fail-open, inspector reason | T25 |
| No embed on chat path; background yield | T25 |
| Superseded not injected as current | T25 |
| No external vector DB | both (no new dependency) |
| Inbox chrome / Remember-this | out of scope |

## Out of scope

Chat-chrome proposal badge, Remember-this, embedding-model discovery dropdown, remote-endpoint memory consent, contradiction review UI, macOS/Linux.

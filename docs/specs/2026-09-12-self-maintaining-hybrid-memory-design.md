# Self-maintaining hybrid memory

Date: 2026-09-12. Status: accepted. Implementation: T24 and T25 in `docs/PLAN.md` and `docs/specs/2026-09-12-self-maintaining-hybrid-memory-plan.md`. Not implemented until those tasks are done.

This spec is the approved direction for memory formation and retrieval.

Related: `docs/PROJECT.md` (current product spec), ADR-004 (lexical-first default), ADR-005 (validated extraction, review for uncertain proposals). Accepting this spec supersedes the *defaults* in those ADRs, not the file-canonical or validate-before-write rules.

## Goal

Characters accumulate durable memories without the user reviewing every proposal. Those memories live in portable Markdown files. The next chat turn retrieves real file text through hybrid lexical and vector search, injected as labeled archival data, so the model is grounded in stored records rather than inventing them.

The user can open, edit, lock, exclude, or delete any memory file. That is optional. Ordinary chat should keep the vault current by itself.

## Non-goals

- External vector databases (Chroma, Qdrant, and similar). Vectors stay a rebuildable SQLite projection of the files.
- The model writing Markdown directly.
- Auto-writing inferred guesses.
- Changing `character.md` via extraction.
- Sending vault memories to non-loopback endpoints without explicit consent.
- Extracting initiative turns as factual memories.
- Voice, group chat, cloud sync, or always-on indexing after Quit.

## Current behavior this spec changes

Already implemented and kept:

- Markdown memories and transcripts are canonical.
- Completed assistant turns enqueue a durable extraction job.
- The model proposes structured candidates; application code validates schema and source-turn IDs.
- FTS5 lexical index and per-character SQLite vectors exist.
- Hybrid ranking exists, with lexical fallback and a context inspector.
- Lock, exclude, delete, pin, and external-edit fingerprint checks exist.
- Deleted memories are suppressed on re-extraction.

Changed by this spec:

- Hybrid retrieval is opt-in (`use_hybrid_retrieval` defaults false). It becomes the default when an embedding model is configured.
- Every proposal stays `needs_review`. “Auto-commit” only saves after the user clicks Accept. Validated user-stated, event, and character/world candidates auto-write. Inferred candidates stay in the inbox.
- Embedding rebuild can run on the chat path when the index is stale. Rebuild and embed become background work that yields to chat.

## Architecture

Two jobs, one source of truth:

1. **Formation.** After a completed user/assistant exchange, a background worker asks the selected chat model for structured candidates. Application code decides whether a Markdown file is written.
2. **Retrieval.** Before the next chat request, the orchestrator searches committed files (lexical + vectors when available), budgets the winners, and injects them as data.

```text
user message
    → hybrid retrieve committed Markdown (lexical fallback if needed)
    → inject labeled archival excerpts into the prompt
    → complete assistant turn
    → enqueue extraction (yields to the next user message)
    → model returns candidates + source IDs
    → validate, dedupe, classify
    → auto-write allowed Markdown / inbox the rest
    → refresh FTS5; enqueue embed of new or changed chunks
```

The chat model does not remember. It is handed excerpts of files. The inspector is how the user sees whether those excerpts were used.

Components (existing, behavior-adjusted):

| Unit | Does | Depends on |
| --- | --- | --- |
| Extraction worker | Proposes candidates from a completed turn; never writes files itself | Provider chat, transcript, durable job queue |
| Validator / reconciler | Schema, source IDs, duplicates, supersession, locks, deletion suppression, inferred-vs-fact split | Vault Markdown, `state.sqlite` |
| Vault | Atomic Markdown create/update; fingerprint conflict detection | Character folder |
| Lexical index | FTS5 over committed memory bodies | Vault Markdown |
| Embedding index | Versioned SQLite vectors over the same chunks | Vault Markdown, embedding provider |
| Retriever | Hybrid rank + budget; fail open to lexical | Both indexes |
| Prompt builder | Inserts winners as archival data with IDs | Retriever, character, scene, history |
| Inspector | Per-turn sources, reasons, token estimates, hybrid vs fallback | Conversation snapshot |

## Formation and commit policy

The extractor may only propose. Application code writes.

Origins already on `ExtractionCandidate`: `user_stated`, `character_fact`, `conversation_event`, `inferred`. Those labels are the commit switch. The model’s origin is untrusted. The validator reclassifies:

- `inferred`, or any candidate the model labeled otherwise without supporting evidence → inbox.
- `user_stated` requires at least one evidence quote from a **user** turn in the completed exchange. Assistant-only “you said…” does not qualify.
- `conversation_event` and `character_fact` require source-turn IDs that exist in that exchange. They still cannot auto-write if the validator would call them a guess.

An assistant sentence is not evidence of a real user event.

| Candidate | Action |
| --- | --- |
| User-stated, shared event, or evidenced character/world fact; schema and source-turn IDs valid; not a duplicate of a live memory | Write a new Markdown file. Refresh FTS5. Enqueue embedding of its chunks. |
| Duplicate of an existing live memory | Link as duplicate. Do not write a second file. |
| Contradicts or supersedes an unlocked memory | Write a new current file. Mark the old file superseded. Retrieval treats only the current file as true. The old file stays on disk for history. |
| Touches a locked memory, or the file’s fingerprint is newer than the job | Inbox. Do not overwrite. |
| Origin `inferred` (a guess, including when source IDs look valid) | Inbox only. Not indexed, not retrieved. |
| Re-extraction of a transcript whose memory was deleted | Stay deleted. No resurrection. |
| Malformed JSON, invented source IDs, empty body, failed schema | Retry the job up to the existing bound, then mark failed. No vault write. |

Inbox items persist in durable operational state across restart. Rejecting an inbox item does not delete the transcript. Accepting an inferred item writes a normal Markdown file; from then on it is a committed memory like any other.

The existing Memories proposal list is the inbox for this spec. A chat-chrome badge, auto-refresh count, and manual **Remember this** stay unscheduled possible features. They are not required to ship this change.

`character.md` remains author-controlled. Extraction cannot edit it.

## Files

One Markdown file per memory, with existing metadata: schema version, stable ID, type, timestamps, source session/turn IDs, topics/entities, salience, confidence, review/retrieval status, lock/exclude/pin.

Users may edit files in the app or externally. External edits are detected by content fingerprint. A newer user edit always wins over a worker. After any successful file change, FTS5 updates immediately and embedding of changed chunks is enqueued.

Portable packs continue to ship Markdown plus durable operational state (inbox, suppression, counters). Rebuildable `index.sqlite` is not the export. Restore rebuilds both indexes from files.

## Retrieval and prompt

**Default.** Hybrid is on when the active provider has a non-empty embedding model. There is no user checkbox required for the default. A setting may still force lexical-only.

**Fail open.** If no embedding model is set, the embed call fails, is cancelled, returns no vector, or the vector index is missing/incompatible/mid-rebuild, use FTS5 only. Record a visible `fallback_reason` on the turn. Do not fail the chat request. Do not claim semantic recall in the inspector.

**Searchable set.** Committed Markdown only: auto-written facts/events/character facts, and inferred items the user later accepted. Not searchable: inbox proposals, rejected proposals, excluded records, deleted records, superseded bodies (the replacement file is current).

Pinned memories remain searchable and consume an explicit budget so they are not dropped when the prompt is full. Locked memories are searchable unless also excluded.

**Turn construction.**

1. Scope candidates to the active character.
2. Lexical search for exact names, quotes, rare tokens.
3. Vector search for paraphrase, when embeddings are available and fresh.
4. Merge with recency, salience, entity/link overlap, and type diversity.
5. Deduplicate by memory ID.
6. Apply pinned budget, then remaining memory budget. Never exceed the reserved model context.
7. Inject winners as a labeled archival-data block with memory IDs and source turn IDs, not as the character’s voice.

Prompt order is unchanged:

1. Application behavior rules (omitted when empty).
2. Character identity and author-defined boundaries.
3. Current relationship or scene context.
4. Retrieved memories as archival data with source IDs.
5. Selected recent conversation turns.
6. Current user message, or a labeled internal initiative event.

**Inspector.** Each assistant turn shows: injected memory files, reasons (lexical, semantic, pinned, recency), estimated tokens used, omitted count, and whether the turn was hybrid or lexical-fallback. That is the distinction between “used the file” and “guessed.”

**Embeddings are a projection.** Store provider, model, dimensions, chunking version, and content fingerprints. Never search mixed embedding spaces. Changing the embedding model invalidates that character’s vectors and rebuilds from files in the background. Chat uses lexical until that rebuild finishes.

## Background work and failures

Chat, retry, character switch, and shutdown take priority over extraction and embedding. Incomplete, failed, or interrupted assistant turns are not extracted. Initiative turns are not automatically extracted as facts.

One candidate is committed only after validation, as an atomic Markdown write. A failed index update after a successful file write is repairable by rebuild; the file remains the authority.

If FTS5 or vectors are corrupt, they are rebuilt from Markdown. An interrupted embed rebuild discards partial vectors and starts from chunk fingerprints.

On restart: pending extraction jobs resume without duplicating memories or resurrecting deletions; an interrupted embed rebuild starts clean; there is no catch-up burst that blocks the first user message.

Wrong auto-written facts are corrected by editing or deleting the file. The next turn retrieves the corrected record. The old text is not silently re-extracted.

## Privacy

Loopback providers (`localhost`, `127.0.0.1`, `::1`) receive vault retrieval. Other endpoints receive none until an explicit “send memories to this endpoint” control exists and is enabled. This spec does not add that control; it keeps the current deny default.

Provider credentials stay in application config and are not exported with the character.

## Tests

Required coverage before marking the work done:

- Auto-write a `user_stated` / `conversation_event` / `character_fact` candidate with real source-turn IDs: Markdown exists; FTS5 hits; vectors enqueue or exist when an embedding model is configured.
- An `inferred` candidate remains inbox-only: no file, not retrieved.
- Duplicate of a live memory: no second file; link recorded.
- Supersession: new file retrieved as current; old file not injected as true.
- Locked file or newer user fingerprint: candidate goes to inbox; file bytes unchanged.
- Deleted memory stays gone after reprocessing the same transcript.
- Hybrid is used when an embedding model is set; lexical plus a recorded fallback reason when embed fails, is missing, or is cancelled.
- Existing paraphrase vs exact-name evaluation fixture still holds under hybrid default.
- A chat send in flight is not blocked by an embedding rebuild.
- Restart drains the extraction queue once and does not duplicate or resurrect.
- External Markdown edit is detected and reindexed; a concurrent worker does not overwrite it.
- Retrieval stays inside the active character.

## Decision updates after acceptance

When this spec is accepted, in the same change as the plan tasks:

- Supersede ADR-004’s “lexical is the default until later.” Lexical remains the required fallback and the only path when embeddings are unavailable. Hybrid is the default whenever an embedding model is configured.
- Update ADR-005: the model still only proposes; application code still validates and writes. Uncertain **inferred** proposals still require review. Validated non-inferred proposals auto-write instead of all remaining `needs_review`.
- Replace `docs/PROJECT.md` memory step 5–6 and the “first usable version must function without embeddings” wording so hybrid-default plus lexical fallback is the specified behavior.
- Move “embedding rebuild and hybrid default” out of possible later features and into `docs/PLAN.md` as scheduled tasks. Inbox chrome and Remember-this stay unscheduled unless explicitly added.

## Success criteria

After implementation, a user can chat without opening Memories and later find supported facts in Markdown files. Inferred guesses wait in the inbox and are not retrieved. The next turn’s inspector shows the files that were injected, or a lexical fallback reason. Editing a file changes subsequent replies. Deleting the search index and restoring the vault from files recovers the same memories.

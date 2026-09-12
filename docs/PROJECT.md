# Project specification

## Goal

Users chat with persistent AI characters through a desktop application. Models run locally through Ollama, llama.cpp, or LM Studio. A character's identity and memories persist independently of a particular model, provider, or installation of the app.

The application should make characters consistent, able to recall relevant experiences, and able to initiate occasional contextually appropriate conversation when enabled.

## Agreed direction

- Desktop shell: Tauri 2.
- Interface: React and TypeScript.
- Application core: Rust.
- Human-readable storage: Markdown with YAML frontmatter and Obsidian-style links.
- Local database: SQLite for searchable/indexed data and application state.
- Model integration: native Ollama adapter and a shared OpenAI-compatible chat/embedding transport with provider-specific capabilities for llama.cpp and LM Studio.
- Retrieval: lexical search first, then hybrid lexical/vector retrieval.
- Initial model operation: connect to an already running local provider and select an available model. Bundling or launching inference engines is a later capability.

Windows is the first development and verification target because this workspace is on Windows. Maintain portable boundaries for macOS and Linux; do not claim those platforms work without testing them.

## User workflows

1. Configure and test a connection to a local provider.
2. Select a chat model and optionally a separate embedding model.
3. Create or load a character from the Characters library, inspect its definition, and begin chatting.
4. Receive streamed replies that use the character identity, recent conversation, and relevant memories.
5. Inspect which memories influenced a response.
6. View, edit, pin, exclude, or delete memories and review uncertain memory proposals.
7. Enable spontaneous engagement and configure quiet hours, cooldowns, and frequency limits.
8. Export or move a character and its memories, then load it on another installation.

## Component boundaries

| Component | Responsibility |
| --- | --- |
| Desktop UI | Character library and identity editor, chat, settings, memory browsing, retrieval inspection |
| Conversation orchestrator | Turn lifecycle, streaming, cancellation, request ordering |
| Character loader | Versioned character schema, validation, portable paths |
| Context builder | Prompt ordering, token budget, history selection, archival labeling |
| Vault service | Markdown parsing/writing, provenance, external edits, import/export |
| Retrieval service | Chunking, full-text search, embeddings, ranking, filtering |
| Memory worker | Candidate extraction, validation, deduplication, review, persistence |
| Initiative scheduler | Eligibility, cooldowns, event generation, cancellation |
| Provider adapters | Health, discovery, chat streaming, embeddings, capability reporting |

Provider capabilities must describe supported features instead of assuming identical behavior across engines. Generation, extraction, and embeddings share a resource scheduler so background work cannot repeatedly evict or delay the active chat model.

## Character vault and data ownership

Suggested portable layout:

```text
characters/<character-id>/
  character.md
  memories/
    people/
    episodic/
    semantic/
    relationships/
    open-threads.md
  chats/
    <session-id>.md
  assets/
    portrait.png
  .tz-chatter/
    index.sqlite
    state.sqlite
```

- `character.md`: author-controlled identity, traits, speech, motivations, and boundaries. Automatic memory extraction cannot edit it.
- `memories/`: authoritative memory records, with stable IDs and Markdown bodies. Obsidian installation is not required.
- `chats/`: authoritative transcripts with stable session and turn IDs, timestamps, roles, and completion/cancellation status. Define an unambiguous round-trip format during T03; rendering alone must not destroy message boundaries.
- `index.sqlite`: rebuildable derived search data, chunks, links, and embeddings. Deleting it must not lose memories or transcripts.
- `state.sqlite`: durable operational state such as pending jobs, review candidates, initiative counters, and suppression of deleted memory proposals. It is not a disposable index and belongs in backup/export behavior.

Any SQLite transcript representation is a projection of canonical transcripts, not an independent writable history. Provider URLs, credentials, and machine-specific preferences live in application configuration, outside portable character identity. Do not export credentials.

This layout is a schema proposal to formalize during T03. Version all persisted formats and provide migrations when they change.

## Memory records and lifecycle

Memory metadata should include schema version, stable ID, type, timestamps, source session/turn IDs, topics/entities, salience, confidence, and review/retrieval status. Model confidence is a heuristic, not a calibrated probability.

Distinguish user-stated facts, fictional character/world facts, shared conversational events, and inferred impressions. An assistant's invented statement is not evidence of a real user event. Track time and supersession so changed preferences need not become unexplained contradictions.

After a completed conversational exchange:

1. Queue extraction using a stable source-turn key.
2. Ask for structured candidate memories with supporting source IDs.
3. Validate the schema, source references, and permitted categories.
4. Compare against existing memories and earlier proposals.
5. Reject trivial duplicates, preserve conflicting evidence, and route uncertain candidates to review.
6. Commit allowed changes atomically, then refresh derived indexes.

Support user correction, locking, exclusion, and deletion. A deleted memory must not be silently reconstructed from its old transcript by a retried extraction job. Define suppression and explicit reprocessing behavior. Deleting a memory does not implicitly erase its transcript; the UI must distinguish those actions.

External Markdown edits must be detected and reindexed. Use content fingerprints or equivalent checks to prevent background work from overwriting newer edits. A failed extraction or index update must not lose the user conversation.

## Retrieval and prompt construction

Retrieval is scoped to the selected character before ranking. Use lexical matching, embedding similarity, entity/link matches, recency, and salience. Deduplicate and diversify the final results. Pinned memory consumes an explicit budget and must not silently exceed the model context.

Markdown is the source of truth; RAG is the retrieval-and-context process, and a vector index is one optional tool within it. The first usable version must function without embeddings.

Record embedding provider/model identity, revision or fingerprint where available, dimensions, chunking version, and content fingerprint. Rebuild incompatible indexes when these change. Never compare vectors from different embedding spaces. Fall back visibly to lexical retrieval if embeddings are unavailable.

Prompt order:

1. Application behavior rules (user-editable in Settings; omitted when empty).
2. Character identity and author-defined boundaries.
3. Current relationship or scene context.
4. Relevant memories labeled as archival data with source IDs.
5. Selected recent conversation turns.
6. Current user message, or a clearly labeled internal initiative event.

Reserve output capacity and account for the whole request. Use provider/model tokenization when supported; otherwise use a documented conservative estimate and recover gracefully from context-limit errors. Expose selected memories, omissions, and budget estimates in the context inspector. Do not manufacture a user message to represent spontaneous engagement.

## Initiative behavior

Run the scheduler inside the desktop core initially, including optional tray operation. Quitting the app stops it. A separate always-running service is deferred.

Eligibility checks precede inference: opt-in, active character, conversational inactivity, quiet hours, cooldown, frequency cap, unanswered initiatives, model availability, and resource policy. Use app conversation activity initially; operating-system-wide activity monitoring is not required.

Eligible events may use unresolved topics or prior memories. The model can choose silence. Generated questions do not count as new factual memories unless later conversation supports them.

Apply backoff after ignored messages, persist counters across restarts, and avoid catch-up bursts after sleep. A new user message, character change, disabled setting, or shutdown cancels stale initiative work before display. Label spontaneous turns and make notification delivery separately configurable.

## Reliability and privacy

- Local storage and inference by default; remote endpoints require explicit configuration. No automatic cloud fallback.
- Do not download models or launch new inference services without a user-visible action.
- Do not log credentials or entire conversations by default in diagnostic output.
- Validate imported paths and assets and confine writes to the selected vault.
- Preserve character isolation even when switching during in-flight requests.
- Treat interrupted replies as interrupted; only complete eligible turns enter normal extraction.
- Recover pending jobs after restart without duplicate memories or repeated unsolicited messages.
- Use backups and conflict handling for durable files; index corruption should be repairable.

## Scope boundaries

The initial release includes text chat, one active character at a time, local provider connections, character vaults, editable memories, retrieval, automatic extraction, and optional initiative.

Voice, avatars with animation, multi-character group chat, cloud synchronization, a character marketplace, training/fine-tuning, arbitrary tools, bundled model downloads, and running when the application is fully closed are deferred. Add them only through an explicit scope decision.

## Success criteria

A user can reconnect after an application restart, resume a character conversation, retrieve an earlier relevant memory with inspectable provenance, correct that memory outside or inside the app, and see subsequent replies use the updated record. They can switch supported providers without rewriting their character vault. With initiative enabled, the character can send a relevant spontaneous message while respecting quiet hours and stopping after ignored attempts according to settings.

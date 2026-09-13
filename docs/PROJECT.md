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
- Retrieval: hybrid lexical/vector search when an embedding model is configured; lexical search otherwise. Vectors are a rebuildable SQLite index over Markdown, not a separate database.
- Initial model operation: connect to an already running local provider and select an available model. Bundling or launching inference engines is a later capability.

Windows is the first development and verification target. Maintain portable boundaries for macOS and Linux. Linux was tested on Ubuntu 24.04.5 LTS GNOME/Wayland (T33, VirtualBox guest); do not generalize that to “Linux is supported.” macOS is intended to compile and may use Cmd in the keymap; do not claim it without a Darwin host launching the window (ADR-017).

## User workflows

1. Configure and test a connection to a local provider.
2. Select a default chat model and optionally a separate embedding model. Provider connection stays app-global.
3. Create or load a character from the Characters library, optionally set that character’s chat model and sampling, inspect its definition, and begin chatting.
4. Receive streamed replies that use the character identity, recent conversation, and relevant memories.
5. Inspect which memories influenced a response.
6. Optionally view, edit, pin, exclude, or delete memories, review inferred guesses, and exclude a stale file from a contradiction or supersession pair. Ordinary chatting writes supported facts without this step.
7. Enable spontaneous engagement and configure quiet hours, cooldowns, and frequency limits.
8. Export or move a character and its memories, then load it on another installation.

## Component boundaries

| Component | Responsibility |
| --- | --- |
| Desktop UI | Character library and identity editor, chat, settings, memory browsing, conflict review, retrieval inspection |
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
  persona.md
  scene.md
  locals/
    cafe.md
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
    generation.json
    index.sqlite
    state.sqlite
```

- `character.md`: author-controlled identity, traits, speech, motivations, and boundaries. Automatic memory extraction cannot edit it.
- `persona.md`: who the user is to this character. Portable Markdown outside identity; extraction cannot edit it.
- `scene.md`: character default local id. Empty means no default scene.
- `locals/`: reusable scene/place Markdown. A local must exist here before a character default or session can select it. Sessions store a live-linked id, not a snapshot.
- `memories/`: authoritative memory records, with stable IDs and Markdown bodies. Obsidian installation is not required.
- `chats/`: authoritative transcripts with stable session and turn IDs, timestamps, roles, and completion/cancellation status. Define an unambiguous round-trip format during T03; rendering alone must not destroy message boundaries.
- `generation.json`: optional per-character chat model, temperature, and max tokens. Not part of portable identity; omitted from packs unless the user later opts into exporting it.
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
3. Validate the schema, source references, and permitted categories. Reclassify origin from transcript evidence: `user_stated` requires a quote from a user turn.
4. Compare against existing memories and earlier proposals.
5. Auto-write validated user-stated facts, shared events, and evidenced character/world facts as **new** Markdown files. Leave inferred guesses in the review inbox. Do not mutate existing memory bodies.
6. Refresh derived indexes from those files. User correction, locking, exclusion, and deletion remain optional.

Support user correction, locking, exclusion, and deletion. A deleted memory must not be silently reconstructed from its old transcript by a retried extraction job. Define suppression and explicit reprocessing behavior. Deleting a memory does not implicitly erase its transcript; the UI must distinguish those actions.

External Markdown edits must be detected and reindexed. Use content fingerprints or equivalent checks to prevent background work from overwriting newer edits. A failed extraction or index update must not lose the user conversation.

## Retrieval and prompt construction

Retrieval is scoped to the selected character before ranking. Use lexical matching, embedding similarity, entity/link matches, recency, and salience. Deduplicate and diversify the final results. Pinned memory consumes an explicit budget and must not silently exceed the model context.

Markdown is the source of truth; RAG is the retrieval-and-context process, and a vector index is a rebuildable projection of those files. Chat must still function without embeddings, via lexical retrieval and a visible fallback reason. Hybrid is the default when an embedding model is configured.

Record embedding provider/model identity, revision or fingerprint where available, dimensions, chunking version, and content fingerprint. Rebuild incompatible indexes when these change. Never compare vectors from different embedding spaces. Fall back visibly to lexical retrieval if embeddings are unavailable.

Prompt order:

1. Application behavior rules (user-editable in Settings; omitted when empty).
2. Character identity and author-defined boundaries.
3. User persona (who the user is; omitted when empty).
4. Current scene from the resolved local (character default unless the session overrides or clears it; omitted when none).
5. Relevant memories labeled as archival data with source IDs.
6. Selected recent conversation turns.
7. Current user message, or a clearly labeled internal initiative event.

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

Engine installation or launching remains deferred. If revisited, it must stay a user-visible action with no hidden downloads or silent cloud fallback.

## Possible later features

These are unscheduled product ideas, not committed work. Self-maintaining hybrid memory is scheduled as T24/T25; do not start the items below as the next implementation task. Promote an item into `docs/PLAN.md` only after an explicit scope decision. Candidates must keep Markdown as the source of truth, leave `character.md` author-controlled, confine writes to the selected vault, and keep provider credentials out of portable packs.

Suggested starting cluster if one group is promoted first: memory inbox, remember-this-from-a-turn, and open-vault-as-files.

### Highest-leverage candidates

- **Memory inbox.** Surface pending extraction proposals on the chat chrome (badge, count, auto-refresh) so review does not require opening Memories and clicking Refresh.
- **Remember this.** Create a Markdown memory from a selected transcript turn with known source IDs, without waiting for the extraction worker.
- **User persona and scene notes.** Scheduled as T26: portable `persona.md` plus a `locals/` library with character default and session override.
- **Character portraits.** Scheduled as T27: render `assets/portrait.png` in the character library, chat sidebar, and identity editor; add via GUI or by dropping the file into the vault.
- **Open vault as files.** Open the character folder, reveal a memory in the file manager, and optionally open the vault in Obsidian.
- **Session names, search, and archive.** Scheduled as T28: auto-title from the first user turn, rename sessions, search transcripts, and archive in place with `archived: true` without deleting the canonical Markdown.
- **Edit, regenerate, and continue.** Scheduled as T29: rewrite the last user turn, request another assistant reply, and continue an interrupted reply. Persist superseded/complete/interrupted outcomes as transcript statuses so extraction does not double-commit.
- **Per-character model and sampling.** Scheduled as T30: optional chat model, temperature, and max tokens per character in `.tz-chatter/generation.json`. Provider URLs and credentials stay app-global. Packs omit generation settings unless a later opt-in export is added.
- **Contradiction and supersession review.** Scheduled as T31: list committed `contradicts`/`supersedes` pairs on Memories and exclude the stale Markdown file. Links stay in durable `state.sqlite3`.
- **First-run provider coach.** Explain unreachable endpoints (“nothing is listening on 11434”) with retry. Give the embedding model the same discovery-backed select as chat models. Add an explicit “send memories to this endpoint” control; remote URLs must keep the current no-disclosure default.
- **Keyboard-first chat.** Scheduled as T32: Ctrl+N new session, Ctrl+K typeahead character switcher, Ctrl+L focus composer, Ctrl+. stop generation, Ctrl+I toggle Sources, Escape dismiss stack. Cmd mirrors Ctrl on macOS without claiming macOS verification.

### On-mission follow-ups

- **Linux verification.** Done as T33 on Ubuntu 24.04.5 LTS GNOME/Wayland (VirtualBox guest): launch, Windows-copied vault round-trip, one chat turn, named deb/AppImage artifacts. That is not a general Linux support claim (ADR-017).
- **macOS.** Intended but unverified. Cmd mirrors Ctrl as a portable binding. No macOS task until a Darwin host exists; do not claim the platform from compile-time portability.

## Success criteria

A user can reconnect after an application restart, resume a character conversation, and have supported facts from earlier chats stored as Markdown and retrieved on later turns without reviewing each proposal. They can still correct a memory outside or inside the app and see subsequent replies use the updated record. They can switch supported providers without rewriting their character vault. With initiative enabled, the character can send a relevant spontaneous message while respecting quiet hours and stopping after ignored attempts according to settings.

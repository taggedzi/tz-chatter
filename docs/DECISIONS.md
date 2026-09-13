# Decision log

Accepted decisions describe project direction, not implemented features. Preserve previous entries when changing direction: mark the old entry superseded and link the replacement. Update PROJECT.md and affected plan tasks accordingly.

## ADR-001 — Fresh project and portable character identity

Date: 2026-09-11. State: accepted. Basis: explicit user request and approved design.

Treat this project independently of previous similar conversations. A character definition and its Markdown memory folder belong to the user and survive provider/model changes. The AI cannot silently rewrite author-controlled character identity.

Consequence: do not bind identity or memory to a model's hidden state or provider-specific saved conversation.

## ADR-002 — Desktop stack and provider boundary

Date: 2026-09-11. State: accepted. Basis: approved architectural proposal.

Use Tauri 2, React/TypeScript, and a Rust core. Support Ollama natively and llama.cpp/LM Studio through compatible transports and provider-specific capabilities. Start with external local servers; engine management is deferred.

Consequence: generation, storage, and scheduling live behind a typed application boundary. Verify provider behavior independently. Windows is the initial development target, with portable code boundaries. Platform *claims* (what README and release notes may say works) are [ADR-017](#adr-017--platform-claims-windows-verified-linux-scheduled-macos-unverified).

Reference APIs to recheck during implementation: [Ollama chat](https://docs.ollama.com/api/chat), [Ollama embeddings](https://docs.ollama.com/api/embed), [llama.cpp server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md), and [LM Studio APIs](https://lmstudio.ai/docs/developer).

## ADR-003 — Markdown authority and SQLite roles

Date: 2026-09-11. State: accepted. Basis: approved Markdown-first design; storage-role clarification made during planning.

Markdown character definitions, memories, and transcripts are canonical. SQLite search and embedding data are rebuildable projections. Durable operational state is stored separately from the disposable index so review decisions, deletion suppression, and scheduler counters survive rebuilds.

Consequence: specify one authoritative representation per data type. Export/backup must preserve durable operational data and exclude machine credentials. The precise schema is T03 work.

## ADR-004 — Retrieval works before embeddings

Date: 2026-09-11. State: superseded by [ADR-009](#adr-009--self-maintaining-hybrid-memory). Basis: approved incremental implementation proposal.

Start with FTS5 lexical retrieval and manually created memories. Add embedding similarity and hybrid ranking after the conversation/memory loop is inspectable. Store vectors in SQLite and begin with direct similarity search; consider a specialized index only after measurement warrants it.

Consequence: no embedding model or separate vector-database service is required for the first usable version. Record embedding-space identity and rebuild incompatible indexes.

Superseded default: lexical-as-product-default. Lexical remains the required fallback. Hybrid is the default when an embedding model is configured (ADR-009).

## ADR-005 — Validated automatic memory formation

Date: 2026-09-11. State: superseded in part by [ADR-009](#adr-009--self-maintaining-hybrid-memory). Basis: approved memory pipeline.

The LLM proposes structured memories; application code validates sources, deduplicates, handles contradictions, and writes files. Preserve source references and user edit/lock/exclude/delete controls. Support review for uncertain proposals.

Consequence: implement durable, idempotent jobs and prevent deletion/retry loops from silently reintroducing removed memories. Assistant inventions cannot become real user facts solely through extraction.

Still in force: propose/validate/write split, deletion suppression, user edit/lock/exclude. Superseded default: every proposal stays `needs_review` until the user accepts. ADR-009 auto-writes validated non-inferred proposals.

## ADR-006 — Bounded, opt-in initiative

Date: 2026-09-11. State: accepted. Basis: approved initiative design.

Use a deterministic eligibility gate before model inference. Enforce quiet hours, cooldowns, caps, ignored-message backoff, and cancellation when user activity makes work stale. The model may choose silence. Start with scheduling inside the app and optional tray operation.

Consequence: quitting stops initiative. Spontaneous turns are labeled and never impersonate user input. Background work yields to chat and does not create unsupported factual memories.

## ADR-007 — File-based project continuity

Date: 2026-09-11. State: accepted. Basis: explicit user request for agent/model handoff.

Use AGENTS.md as the entry point, PROJECT.md as durable specification, PLAN.md as task definitions, STATUS.md as the sole current-state tracker, and HANDOFF.md as chronological history. Use stable task IDs, explicit dependencies, acceptance criteria, and evidence-backed completion.

Consequence: implementation work includes updating state and handoff notes. No external project-management service, prior conversation, or model-specific memory is required to resume work.

## ADR-008 — Character library and application prompt

Date: 2026-09-12. State: accepted. Basis: explicit user request after T20 layout consolidation.

Provide a Characters primary view that lists known vaults, creates new portable character folders, and edits author-controlled `character.md` identity. Persist the library in application configuration. The global application prompt lives in Settings, is stored outside character vaults, and is prepended to chat and initiative requests before character identity and memories. Empty saved text omits that extra message. Automatic extraction still cannot edit character identity.

Consequence: character identity remains portable Markdown. Application behavior rules are machine-scoped and survive character switches. Creating a character writes a new vault rather than overwriting an existing `character.md`.

## ADR-009 — Self-maintaining hybrid memory

Date: 2026-09-12. State: accepted. Basis: user-approved design in `docs/specs/2026-09-12-self-maintaining-hybrid-memory-design.md`.

The user is not required to manage memories. Chatting is sufficient. After a completed turn, the model proposes; application code classifies origin from transcript evidence and writes **new** Markdown files for validated user-stated facts (must quote a user turn), conversation events, and character/world facts. Inferred guesses stay in the Memories inbox and are not retrieved until accepted. Workers never mutate an existing memory body.

Retrieval is hybrid by default when an embedding model is configured. Vectors remain a rebuildable SQLite projection of the files, not an external database. If embeddings are missing, stale, or fail, chat uses lexical search and records that in the inspector. Embedding rebuild runs in the background and yields to user chat.

Consequence: T24/T25 implement this. ADR-004’s lexical product default and ADR-005’s all-proposals-need-review default are superseded. File-canonical storage, character isolation, and “assistant text is not a user fact” remain.

## ADR-010 — User persona and reusable locals

Date: 2026-09-12. State: accepted. Basis: explicit user request to implement the persona/scene backlog item, with session override via a `locals/` library.

Store who the user is as portable `persona.md` in the character vault, not in `character.md` and not in app config. Store reusable scene documents as Markdown files under `locals/`; a local must exist in that folder before it can be selected. `scene.md` records the character default local. A session follows that default, selects another existing local, or clears the scene. Resolution is a live link: each send reads the current files. Automatic extraction cannot write persona, locals, or character identity.

Prompt order inserts labeled user persona after character identity and labeled current scene after persona. Empty files omit those messages. Packs include `persona.md`, `scene.md`, and `locals/*.md`. Locals are not memory-index records.

Consequence: T26 implements this. Application rules remain machine-scoped (ADR-008). Character identity remains author-controlled Markdown.

## ADR-011 — Portable character portraits

Date: 2026-09-12. State: accepted. Basis: explicit user request to implement portraits from the open-vault-as-files backlog item, without folder/Obsidian actions.

The canonical portrait is `{vault}/assets/portrait.png`. The Characters library list, chat sidebar active-character card, and identity editor render it when present and fall back to initials when absent. Users add or replace it from the editor (PNG picker or drag-drop) or by placing that file in the vault folder. Remove deletes only that file. New vaults scaffold an empty `assets/` directory. Writes stay confined to the selected vault; non-PNG and files larger than 5 MB are rejected. Packs include the portrait when it is a valid PNG. Extraction cannot write this path.

Consequence: T27 implements this. Open-folder / Reveal / Obsidian remain unscheduled. JPEG/WebP conversion and animated avatars stay deferred.

## ADR-012 — Session titles and in-place archive

Date: 2026-09-12. State: accepted. Basis: explicit user request to implement the session names/search/archive backlog item, with archived chats staying in `chats/`.

Display names live in transcript YAML as `title`, not in the filename. `{session_id}.md` remains the stable path for resume, extraction, initiative, and packs. Missing `title` is untitled; the first user turn is used as a display title and is persisted on the next transcript save unless a non-empty title already exists. Rename writes `title` and is never auto-overwritten.

Archive is `archived: true` on the same file. Do not move chats into a subdirectory and do not delete Markdown when archiving. The default session list hides archived chats; search includes them. Transcript search reads the canonical Markdown already loaded for the session list; it is not a separate index.

Consequence: T28 implements this. Session identity stays the UUID filename. Memory FTS5 remains for memories only.

## ADR-013 — Last-exchange edit, regenerate, and continue

Date: 2026-09-12. State: accepted. Basis: explicit user request to implement the edit/regenerate/continue backlog item, with the recommended last-exchange design.

Operate only on the last user/assistant exchange. Rewrite the last user turn in place. Regenerate marks the previous assistant `superseded` and writes a new assistant id. Continue appends to an Interrupted reply with the same turn id. Retry stays for failed or empty replies. `superseded` is a normal transcript status; schema stays 1. Prompt history, chat bubbles, and extraction consider only Complete turns. A pending extraction job whose assistant source is no longer Complete is abandoned without a vault write. Memories already committed from a superseded reply are left in place.

Consequence: T29 implements this. No swipe carousel, no editing older turns, no continue of Complete/max-token replies, and no automatic deletion of memories from superseded replies.

## ADR-014 — Per-character model and sampling

Date: 2026-09-13. State: accepted. Basis: explicit user request to implement per-character model and sampling, with per-character overrides only and the recommended extraction split.

Provider connection remains app-global: kind, endpoint, credentials, and embedding model. Settings → Provider chat model is the inherit default, not the only model. Each character may store an optional chat model, temperature, and max tokens in `{vault}/.tz-chatter/generation.json`. Empty fields inherit the active provider. Named presets are deferred.

Do not write these values into `character.md`. Portable packs omit `generation.json` by default so a character can move without carrying machine-specific model names; opt-in export is a later task.

Chat send/retry/regenerate/continue/edit and initiative resolve the override in Rust at request time and send sampling on the provider payload. Extraction uses the character’s resolved chat model so it can stay on already-loaded weights, but ignores character sampling and uses conservative defaults (temperature 0, a dedicated max-token bound). Embeddings stay on the global embedding model. The existing one-at-a-time model gate is unchanged.

Consequence: T30 implements this. A second preset catalog, pack opt-in for generation settings, and per-character embedding models are out of scope.

## ADR-015 — Contradiction and supersession review

Date: 2026-09-13. State: accepted. Basis: explicit user request to implement the on-mission conflict list, with exclude-the-stale-file as resolve and user-picked contradicts sides.

The Memories panel lists committed `contradicts` and `supersedes` links already stored in durable `.tz-chatter/state.sqlite3`. Membership requires both Markdown files to exist and neither to be excluded. `related` links are not disagreements and stay off the list. Resolve is user-initiated exclude of one file; workers still do not mutate existing bodies. Contradicts sides are chosen by the user. Supersedes exclude-older on this path may only target `to_memory_id`. A pair leaves the list when either side is excluded or deleted. No keep-both acknowledgement table. Links are not written into Markdown frontmatter in this task.

Consequence: T31 implements this. Chat-chrome badges, delete-from-list, and auto-exclude of locked files are out of scope.

## ADR-016 — Keyboard-first chat

Date: 2026-09-13. State: accepted. Basis: explicit user request to implement the on-mission keyboard-first chat item, with a typeahead character switcher and the recommended Windows keymap.

The existing chat loop stays mouse-reachable. A global keymap drives new session, character switch, composer focus, stop generation, and Sources. Escape closes in order: character switcher, Settings, in-flight generation, Sources, then composer focus. Session rename keeps Escape to cancel. Ctrl+. always stops a stream when one is running. Cmd mirrors Ctrl on macOS; that is a portable binding, not a macOS support claim.

The character switcher is a typeahead overlay over the existing library list. Choosing a character uses the current vault-resume path, stays on Chat, and focuses the composer. It does not open the identity editor. No command palette, session picker, or user-editable bindings in this task.

Consequence: T32 implements this. Customizable shortcuts, a cheat-sheet modal, and edit/regenerate/continue keys remain out of scope.

## ADR-017 — Platform claims: Windows verified, Linux scheduled, macOS unverified

Date: 2026-09-13. State: accepted. Basis: explicit user request to split the combined macOS/Linux verification growth item. Linux can be tested; no macOS host is available.

Windows remains the verified development and release target ([ADR-002](#adr-002--desktop-stack-and-provider-boundary), T18). The character vault is already a portable Markdown layout. Portable code boundaries, including Cmd mirroring Ctrl in the keymap, are not a platform support claim.

Linux desktop verification is scheduled as T33. Claim Linux only after a recorded launch, a vault round-trip from Windows, and one chat turn on a Linux host. Record the distro/desktop and the packaging artifacts that were actually produced. WSL2/WSLg is not a general Linux claim.

macOS remains intended but unverified. Do not schedule a macOS task, ship a Mac download, or list macOS prerequisites until a Darwin host can launch the window. Compile-only evidence or a later CI runner still does not claim the platform.

Consequence: T33 is Linux-only. README and product copy keep Windows as the verified target. Cmd bindings stay documented as portable, not tested on Mac. No GitHub Actions or notarization work is part of this decision.

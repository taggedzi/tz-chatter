# Decision log

Accepted decisions describe project direction, not implemented features. Preserve previous entries when changing direction: mark the old entry superseded and link the replacement. Update PROJECT.md and affected plan tasks accordingly.

## ADR-001 — Fresh project and portable character identity

Date: 2026-09-11. State: accepted. Basis: explicit user request and approved design.

Treat this project independently of previous similar conversations. A character definition and its Markdown memory folder belong to the user and survive provider/model changes. The AI cannot silently rewrite author-controlled character identity.

Consequence: do not bind identity or memory to a model's hidden state or provider-specific saved conversation.

## ADR-002 — Desktop stack and provider boundary

Date: 2026-09-11. State: accepted. Basis: approved architectural proposal.

Use Tauri 2, React/TypeScript, and a Rust core. Support Ollama natively and llama.cpp/LM Studio through compatible transports and provider-specific capabilities. Start with external local servers; engine management is deferred.

Consequence: generation, storage, and scheduling live behind a typed application boundary. Verify provider behavior independently. Windows is the initial development target, with portable code boundaries.

Reference APIs to recheck during implementation: [Ollama chat](https://docs.ollama.com/api/chat), [Ollama embeddings](https://docs.ollama.com/api/embed), [llama.cpp server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md), and [LM Studio APIs](https://lmstudio.ai/docs/developer).

## ADR-003 — Markdown authority and SQLite roles

Date: 2026-09-11. State: accepted. Basis: approved Markdown-first design; storage-role clarification made during planning.

Markdown character definitions, memories, and transcripts are canonical. SQLite search and embedding data are rebuildable projections. Durable operational state is stored separately from the disposable index so review decisions, deletion suppression, and scheduler counters survive rebuilds.

Consequence: specify one authoritative representation per data type. Export/backup must preserve durable operational data and exclude machine credentials. The precise schema is T03 work.

## ADR-004 — Retrieval works before embeddings

Date: 2026-09-11. State: accepted. Basis: approved incremental implementation proposal.

Start with FTS5 lexical retrieval and manually created memories. Add embedding similarity and hybrid ranking after the conversation/memory loop is inspectable. Store vectors in SQLite and begin with direct similarity search; consider a specialized index only after measurement warrants it.

Consequence: no embedding model or separate vector-database service is required for the first usable version. Record embedding-space identity and rebuild incompatible indexes.

## ADR-005 — Validated automatic memory formation

Date: 2026-09-11. State: accepted. Basis: approved memory pipeline.

The LLM proposes structured memories; application code validates sources, deduplicates, handles contradictions, and writes files. Preserve source references and user edit/lock/exclude/delete controls. Support review for uncertain proposals.

Consequence: implement durable, idempotent jobs and prevent deletion/retry loops from silently reintroducing removed memories. Assistant inventions cannot become real user facts solely through extraction.

## ADR-006 — Bounded, opt-in initiative

Date: 2026-09-11. State: accepted. Basis: approved initiative design.

Use a deterministic eligibility gate before model inference. Enforce quiet hours, cooldowns, caps, ignored-message backoff, and cancellation when user activity makes work stale. The model may choose silence. Start with scheduling inside the app and optional tray operation.

Consequence: quitting stops initiative. Spontaneous turns are labeled and never impersonate user input. Background work yields to chat and does not create unsupported factual memories.

## ADR-007 — File-based project continuity

Date: 2026-09-11. State: accepted. Basis: explicit user request for agent/model handoff.

Use AGENTS.md as the entry point, PROJECT.md as durable specification, PLAN.md as task definitions, STATUS.md as the sole current-state tracker, and HANDOFF.md as chronological history. Use stable task IDs, explicit dependencies, acceptance criteria, and evidence-backed completion.

Consequence: implementation work includes updating state and handoff notes. No external project-management service, prior conversation, or model-specific memory is required to resume work.

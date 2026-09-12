# Completion audit

Date: 2026-09-12

This audit was requested after an earlier model reported the project as complete. `docs/STATUS.md` remains the authoritative task-state source; this file records the discovered release gaps and their remediation evidence without rewriting task history.

## Findings

| ID | Severity | Finding | Required resolution | State |
| --- | --- | --- | --- | --- |
| A01 | high | Editing a memory selected from search reconstructs a partial/default record and can truncate the Markdown body or erase metadata. | Resolve search hits to the complete canonical memory before editing; add regression coverage. | done |
| A02 | high | Conversation and memory commands can trust a frontend character snapshot without validating it against `character.md`; the composer can send before a vault is loaded. | Require an existing vault, validate the canonical character at every character-scoped command, and gate the composer on a successful load. | done |
| A03 | high | The memory UI and shell identity are hard-coded to Lyra. | Use the active persisted vault/character context throughout the UI. | done |
| A04 | high | Embedding storage and hybrid ranking exist only as disconnected modules/tests. | Connect explicit embedding-model and hybrid settings to indexed retrieval with visible lexical fallback. | done |
| A05 | high | Provider streams are accumulated in Rust and delivered to React only after completion. | Deliver incremental typed stream events over the Tauri boundary and render partial replies. | done |
| A06 | high | Extraction runs one independently-cancelled job per completed chat, does not automatically drain/recover on resume, and bypasses shared model scheduling. | Add cancellable foreground-priority model coordination and a draining restart-resumable worker. | done |
| A07 | high | Lexical search requires every natural-language query term, causing relevant memories to disappear for ordinary questions. | Use resilient token selection/matching and cover natural-language retrieval. | done |
| A08 | medium | Initiative quiet hours are fixed in the UI and interpreted as UTC; resume suppression is not wired to runtime resume. | Add local-time controls/offset handling and record restart/sleep resume suppression. | done |
| A09 | medium | Scheduler-delivered initiative turns do not update the open UI and the same content is persisted as both initiative and assistant turns. | Emit delivered outcomes to the active UI and persist one clearly labelled spontaneous turn. | done |
| A10 | medium | The Context navigation item has no rendered view and the inspector lacks source opening, omissions, and budget allocation. | Remove the dead route or implement it; enrich the in-conversation inspector and add safe source navigation. | done |
| A11 | medium | Tray interaction, notifications, and keyboard behavior lack actual Windows interaction evidence. | Implement recoverable close-to-tray behavior and complete an actual interaction check when tooling is available. | done |
| A12 | medium | Strict Clippy validation fails and retrieved-memory prompt separators contain literal `\\n` text. | Correct the prompt formatting and make strict Clippy pass. | done |
| A13 | high | Nearly all application source files are untracked in Git. | Commit the reviewed implementation and documentation while preserving unrelated files. | done |
| A14 | high | Prompt construction rejects oversized input but does not recover from a provider-reported context-window error as required by T06. | Retry once with a reduced bounded prompt when the provider identifies a context/token limit. | done |
| A15 | high | Changing an existing memory's type through the editor writes the new path without removing the original, leaving duplicate divergent records. | Carry the original identity through the command boundary and perform a conflict-checked rename. | done |

## Evidence (2026-09-12)

`cargo test --manifest-path src-tauri/Cargo.toml` — 79 passed, 2 ignored. `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` passed. `npm run lint`, `npm run typecheck`, and `npm run build` passed. `npx tauri build` produced MSI 4,485,120 bytes and NSIS 3,294,811 bytes. Packaged `tz-chatter.exe` launched twice with `MainWindowTitle=tz-chatter` and `Responding=True`, then those processes were stopped.

- A01: `memory_search` returns `search_records` (canonical Markdown). Test: `search_records_return_the_complete_canonical_memory`.
- A02: `open_character_vault` requires a vault and matching `character.md`; empty character ids are rejected; composer Send is disabled until `loadedVaultRoot` matches. Tests: `character_scoped_commands_reject_a_mismatched_vault`, `send_rejects_an_unloaded_character_snapshot`.
- A03: Shell/conversation identity follows `activeCharacterName()` / loaded vault character; path placeholders are generic `character-vault`.
- A04: Conversation settings expose embedding model + hybrid checkbox; `build_hybrid_retrieval` falls back to lexical with a visible `fallback_reason`.
- A05: `conversation_send`/`retry` use a Tauri `Channel`; UI applies `delta` events. Test: `send_streaming_emits_partial_text_before_completion`.
- A06: Resume schedules a drain loop; foreground chat cancels background work and defers jobs. Tests: `extraction_worker_drains_pending_jobs_after_resume`, `foreground_work_cancels_background_model_work`, `foreground_preemption_defers_without_consuming_an_attempt`.
- A07: FTS query drops stop words and falls back from AND to OR. Tests: `natural_language_queries_do_not_require_stop_words`, `search_falls_back_to_any_meaningful_term_when_all_terms_miss`, `natural_language_queries_still_retrieve_matching_memories`.
- A08: Quiet hours use `utc_offset_minutes` from the local timezone; scheduler records resume suppression after a gap. Tests: `quiet_hours_apply_the_supplied_local_offset`, `resume_suppression_prevents_sleep_catch_up_burst`.
- A09: Delivered initiative persists one `TurnRole::Initiative` turn and emits `initiative-delivered`. Test: `initiative_delivery_is_labeled_and_does_not_create_a_fake_user_turn`.
- A10: Context nav item removed. Inspector shows omissions/budget and opens Markdown through a confined `memory_source_path`. Test: `memory_source_navigation_is_limited_to_existing_markdown_memories`.
- A11: Close-to-tray is implemented (`hide_window_instead_of_closing`) and unit-tested. Live tray/keyboard Computer Use helper is unavailable in this environment; that limit is recorded rather than treated as a silent pass.
- A12: `render_memory_context` uses real newlines. Test: `includes_retrieved_memory_with_an_explicit_source_label`. Strict Clippy passed.
- A13: Reviewed `src/`, `src-tauri/` sources, lockfiles, `examples/`, frontend config, and continuity docs committed on local `main` as `6a08a51`; `target/`, `dist/`, `node_modules/`, and `.vscode/` left untracked.
- A14: Provider context-limit errors retry once with `PromptBudget { context_tokens: 2048, reserved_output_tokens: 512 }`. Test: `provider_context_limit_retries_once_with_a_smaller_prompt` asserts the retry request is smaller.
- A15: `memory_upsert` carries original type/id and `rename`s. Test: `memory_upsert_moves_an_existing_record_without_leaving_a_duplicate`.

## Completion rule

T19 is complete only when every finding above is resolved with evidence, the complete validation set passes, Windows packages rebuild, the executable launches, and environment-limited interaction checks are recorded honestly.

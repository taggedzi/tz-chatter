# Sherlock Holmes demonstration evaluation

Date: 2026-09-15. Task: T42. This document records scoped evidence; it is not certification of a model or the full application release.

## Delivered

The embedded pack contains 23 playable files: identity, persona, default scene, three locations, sixteen cited background memories, and an original PNG portrait. Separate author guides cover detailed characterization, original sample dialogue, source register, artwork provenance, and a repeatable demo. The Characters featured entry creates a user-owned copy through the real pack importer. It requires no checkout or network access. No model settings, transcripts, personal user facts, or initiative opt-in are seeded.

## Deterministic and UI evidence

- cargo test --locked --manifest-path src-tauri/Cargo.toml --lib: 184 passed, 4 ignored (the existing 3 live checks and the new opt-in Holmes test).
- Five Holmes integration scenarios cover embedded/source byte equality, real parser validation, source provenance, all three scenes, 23-file export/import, existing/unrelated destination refusal, library-failure recovery, retrieval, disk reopen, index rebuild, and isolation from another vault.
- Five canonical queries retrieve their intended record and retain its citation. Across three scenes, default base input estimates are 2,337–2,413 tokens; with actual retrieval they are 2,600–2,918. The 4,096-token budget reserves 768 for output, leaving 410–728 estimated tokens for history in these examples. Long histories are trimmed by the existing prompt builder. Estimates use the application's conservative character-based estimator.
- 49 frontend tests pass. ESLint, TypeScript, Vite build, Rust formatting, and strict Clippy over all targets pass.
- node docs/review/holmes_ui_review.mjs: real React UI with synthetic IPC, at 1280×800 and 900×620. Portrait loads, card stays within the viewport, folder/Browse controls exist, the typed installation request carries the parent folder, conflicts are visible, and the form recovers after failure. Zero browser runtime errors. This does not exercise a native folder dialog or native Rust IPC.
- Full cargo test attempted to replace the running debug executable and hit Windows error 5. The same integration scenarios were placed in the library test target, which runs without replacing the user's open application. No application process was stopped.

See [UI results](holmes-ui-results.json), [desktop render](holmes-featured-1280.png), and [compact render](holmes-featured-900.png).

## Real local-model runs

All use already-installed models through native Ollama at 127.0.0.1:11434, temperature 0.4, 384 chat output tokens, no embeddings, and disposable synthetic data. The real ConversationService enqueues completed-turn extraction, the real worker validates and commits Markdown, and a new service with a new transcript tests recall after reopening disk state. Extraction uses the app's own conservative sampling. No manual insertion of the test token was performed.

| Run | Actual result | Important observations |
| --- | --- | --- |
| Initial qwen2.5:7b | Automatic token write and new-session recall passed | Incorrectly addressed the visitor as Watson; extra follow-up questions |
| Revised qwen2.5:7b | Failed token persistence | Visitor-addressing error absent; only lore was saved; recall phase did not run |
| llama3.1:8b comparison | Token saved and retrieved, but answer denied it | Invented Mycroft involvement at Norbury despite Watson in the source |
| qwen2.5:7b after continuity clarification | Automatic write and recall passed | First-contact defaults now defer to sourced conversation history |
| qwen2.5:7b final character and scene | Automatic write and recall passed | Correct Norbury theme and exact novel token; remaining style issues below |

These runs span changes to the pack, so they are not a statistical success rate. Preserve all reports: [initial](holmes-live-initial.json), [revised failure](holmes-live-qwen-revised.json), [Llama comparison](holmes-live-llama.json), [continuity pass](holmes-live-continuity.json), [final pass](holmes-live-final.json).

The final token was copper-28b497c7. It was absent from the seed pack, generated per run, committed with source-turn references, selected in the recall request, and correctly repeated in a new two-turn transcript. The final Norbury reply did not address the visitor as Watson. Correspondence did not invent the user's clothes, but referred to ink and paper rather than plainly emphasizing text-only information.

## Limits and quality findings

The pack is complete and functional; the local model remains fallible. In the final run it used an unsolicited gendered address, claimed it would remember before storage completed, and asked more follow-up questions than requested. It also generated a new lore memory containing a question. The example demonstrates why an assistant's memory claim is not evidence that extraction succeeded. Lexical queries can select tangential notes sharing terms such as silver or case. Source inspection remains useful even when the answer sounds convincing.

No final pack claim depends on perfect role fidelity. The live checks assert transport completion, source selection, automatic persistence, and exact token recall. They do not automatically grade all dialogue quality. Do not present the passing test name as a perfect portrayal score.

Not run: packaged native installation clicks, an actual desktop-process restart, live supersession/correction, live initiative/tray/notifications, semantic retrieval with an embedding model, or other operating systems. These remain the operator-demo/release-gate work documented elsewhere. Source review covers the named Doyle passages, not worldwide legal clearance.

## Repeat the live check

Run cargo test --locked --manifest-path src-tauri/Cargo.toml --lib holmes_live_chat_extraction_and_new_session_recall -- --ignored --nocapture with local qwen2.5:7b already installed. HOLMES_TEST_MODEL can name another already-installed local model. The test uses temporary vaults and does not change application settings. Follow the [operator script](../../examples/characters/sherlock-holmes/DEMO.md) for a visible desktop demonstration and score each capability separately.

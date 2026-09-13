This is a product-ideation question, not a build request — recommendations 
only, no spec or code until you pick something.

The core loop is already there: local models, portable Markdown characters, 
retrieval, extraction, optional initiative. The features that would actually 
help users are the ones that make that loop easier to live with, not a second 
product bolted on.

## Highest leverage

These close daily friction and lean on work that already exists.

1. **IMPLEMENTED** Memory inbox as a real product surface. Extraction already queues 
review-only proposals, but you have to go to Memories and refresh. A badge, 
auto-refresh, and “3 memories to review” on the chat chrome would make the 
unique pipeline visible. Pair it with Remember this on a turn: skip the model 
and write a Markdown memory from the selected message, with source IDs already 
known.

2. **IMPLEMENTED** User persona and scene notes. Prompt order already has application rules, 
character identity, and “current relationship or scene context,” but users 
still have no first-class “who I am” or “we are at the cafe tonight” fields. 
Character chat lives or dies on that. Keep them Markdown, portable, and outside 
character.md so extraction still cannot rewrite identity.

3. Open the vault as files. The selling point is that memories are ordinary 
Markdown. Open folder, Open in Obsidian, and Reveal this memory would make that 
real. **IMPLEMENTED (T27)** Portraits from assets/portrait.png in the library, 
chat sidebar, and identity editor.

4. **IMPLEMENTED (T28)** Session names, search, and archive. Auto-title from
the first user turn, rename, search transcripts, and archive in place with
`archived: true` without deleting the Markdown.

5. **IMPLEMENTED (T29)** Edit last message, regenerate, continue. Rewrite the
last user turn, request another assistant reply, and continue an interrupted
one. Persist those as normal transcript statuses so extraction does not
double-commit. No swipe carousel in this task.

6. **IMPLEMENTED (T30)** Per-character model and sampling. Provider settings stay
app-global. Optional chat model, temperature, and max tokens live in
`.tz-chatter/generation.json`, not `character.md`. Named presets and opt-in
pack export remain later.

7. First-run provider coach. The app connects to an already-running server and 
does nothing if Ollama or LM Studio is down. A health panel that says “nothing 
is listening on 11434” with retry, plus an embedding-model dropdown like the 
chat-model list, would remove the biggest new-user stall. Explicit send 
memories to this endpoint belongs here too: remote URLs currently get no vault 
context, which is correct as a default and invisible as a behavior.

8. **IMPLEMENTED (T25)** Background embedding rebuild and hybrid-as-default.
Hybrid ranking is the default when an embedding model is set and the
per-character index is ready. A stale index lexical-falls-back on send
without embedding, records that in the inspector, and a background worker
rebuilds while yielding to chat. Settings still offers opt-out. A dedicated
index-freshness panel was not part of T25; live embedding quality was not
measured.

9. **IMPLEMENTED (T31)** Contradiction / supersession review. Memories shows
committed `contradicts` and `supersedes` pairs. Exclude the stale Markdown
file (user picks the contradicts side; supersedes excludes the older file).
Both files stay on disk; retrieval drops the excluded one. `related` links
are not listed.

10. **IMPLEMENTED (T32)** Keyboard-first chat. Ctrl+N new session, Ctrl+K
typeahead character switcher, Ctrl+L focus composer, Ctrl+. stop generation,
Ctrl+I toggle Sources, Escape dismisses switcher / Settings / generation /
Sources then focuses the composer. Mouse paths stay. Cmd mirrors Ctrl on
macOS without claiming macOS verification.

## Next, still on-mission

• macOS / Linux verification. The vault format is already portable; the missing 
piece is the app itself on the other machines people would move a character to.

## I would keep deferred

Voice, animated avatars, group chat, a marketplace, cloud sync, fine-tuning, 
arbitrary tools, bundled model downloads, and running after Quit. Those either 
fight local-first ownership or blow up the one-active-character, 
Markdown-canonical design. Engine launch (“start Ollama for me”) is the one 
deferred item I would revisit early, as a user-visible action, because 
first-run currently assumes too much.

# Contradiction and supersession review

Date: 2026-09-13. Status: accepted. Implementation: T31 in `docs/PLAN.md` (done).

This spec is the proposed direction for a vault-honesty list over links the reconciler already stores. It does not change extraction, auto-write, or hybrid ranking.

Related: `docs/PROJECT.md` (T31), T11 (relationship links), T12 (proposal inbox), ADR-003 (Markdown vs durable state vs disposable index), ADR-009 (new files, no body mutation), ADR-015.

## Goal

Users can open Memories and see committed memories the reconciler has linked as `contradicts` or `supersedes`. They can exclude the stale side so retrieval stops using it. Both Markdown files remain. Ranking no longer has to hide the disagreement.

Ordinary chat still writes supported facts without this step. Reviewing conflicts is optional, like editing or locking a file.

## Non-goals

- Chat-chrome badge, auto-refresh, or a separate Conflicts route.
- A Keep-both dismiss / acknowledgement table.
- Deleting from this list (Delete stays on the existing editor).
- Writing relationship links into Markdown frontmatter.
- Showing `related` links (those are not disagreements).
- Resolving inferred inbox proposals (those stay in the Review queue).
- Auto-excluding locked memories, or any worker-initiated exclude.
- Mutating memory bodies, merging two files into one, or changing `character.md`.
- Re-deriving links from file text. The list reads stored relationships only.

## Current behavior this spec keeps

- Reconciler commits **new** Markdown files and records `contradicts` / `supersedes` / `related` in durable `.tz-chatter/state.sqlite3` (`memory_relationships`). Those links survive FTS rebuilds. They are not in the disposable memory index.
- Retrieval already skips `supersedes` targets. Excluded memories are omitted from FTS search. Browse still lists excluded and locked files.
- Lock, exclude, delete, pin, and fingerprint conflict checks already exist on the editor.
- Workers never rewrite an existing memory body.

## Product rules

Approved 2026-09-13:

1. **Resolve = exclude.** The chosen file stays on disk with `review_status: excluded`. Retrieval and FTS drop it. The other file is unchanged.
2. **Contradicts:** the user picks which side to exclude. The app does not guess from timestamps or link direction.
3. **Supersedes:** one control excludes the older side (`to_memory_id`, the superseded file).
4. **Leave the list** when either side is excluded or deleted. There is no Keep-both record. A pair the user wants both of stays visible until one side is excluded or deleted.

Additional rules:

5. Both sides must still exist as Markdown in the loaded vault. A missing side drops the row.
6. `related` rows never appear.
7. User-initiated exclude of a **locked** memory is allowed. Nothing auto-excludes a locked file.
8. Clicking a side opens it in the existing editor. Edit, lock, pin, and delete stay there.
9. Character isolation: only the loaded vault. Another character’s state is never listed or written.

## Architecture

One read command and one constrained write. The UI is a section on the existing Memories panel.

```text
Memories Refresh
    → memory_conflict_pairs
        ExtractionQueue lists memory_relationships
        MemoryStore.list joins Markdown
        drop related, missing sides, excluded sides
    → render Conflicts

Exclude older / Exclude this
    → memory_exclude_conflict_side
        verify the relationship still exists
        verify the target id is one side of that pair
        load current Markdown (not a stale UI copy)
        set review_status excluded
        MemoryStore::update (fingerprint check, then save + FTS sync)
    → refresh Conflicts and the browse list
```

| Unit | Does | Depends on |
| --- | --- | --- |
| `ExtractionQueue` | List `contradicts`/`supersedes` rows; unchanged `record_relationship` | `state.sqlite3` |
| Conflict join (Rust core) | Attach both `MemoryRecord`s; filter membership | Queue + `MemoryStore` |
| `memory_exclude_conflict_side` | Exclude one side of a still-open pair | Vault Markdown, fingerprint check |
| Memories UI | Conflicts section; reuse editor for inspect/delete | Typed commands |

Links stay in durable operational state. This task does not make Markdown the source of truth for relationships. A pack or folder copy that omits `.tz-chatter/state.sqlite3` will not show historical conflicts. That is existing T11/T17 behavior.

## Commands and types

Typed Tauri boundary. Names below are the intended wire names.

### `memory_conflict_pairs(vault_root, character_id) -> MemoryConflictPair[]`

Opens the loaded character vault only.

Each pair:

- `relation`: `"contradicts"` | `"supersedes"`
- `from`: side for `from_memory_id` (the newer committed memory)
- `to`: side for `to_memory_id` (related / superseded target)
- `proposal_id`, `created_at`: from the relationship row

Each side:

- `id`, `memory_type`, `review_status`, `locked`, `updated_at`
- `excerpt`: first 280 characters of body, whitespace collapsed
- Full body is **not** required on the list. The editor loads it from the existing browse set or a subsequent select.

Membership filter, applied in this order:

1. `relation` is `contradicts` or `supersedes`.
2. `from_memory_id != to_memory_id`.
3. Both IDs resolve to Markdown in this vault.
4. Neither side has `review_status == excluded`.

Sort: `created_at` descending, then `from_id`, `to_id`, `relation` for stability.

The command does not write. It does not return `related` rows.

### `memory_exclude_conflict_side(vault_root, character_id, from_id, to_id, relation, target_id)`

- `relation` must be `contradicts` or `supersedes`.
- `target_id` must be `from_id` or `to_id`.
- For `supersedes`, `target_id` **must** be `to_id`. The UI only offers Exclude older; the command enforces the same rule so a crafted invoke cannot exclude the newer file through this path. Users can still exclude the newer file from the editor.
- For `contradicts`, `target_id` may be either side.
- The relationship row must still exist.
- Both Markdown files must still exist and neither may already be excluded (same membership as the list). If the pair would not appear on the list, return an error and write nothing.
- Load the target record from disk, set `review_status` to excluded, keep body and other fields, `MemoryStore::update`.
- Locked target: allowed.
- External fingerprint mismatch: existing update error, no write.
- After success the pair must disappear from `memory_conflict_pairs`. FTS must not return the excluded body.

Do not add a generic “exclude by id” that skips relationship checks. Editor exclude continues to use `memory_upsert`.

## UI

`MemoryPanel` gains a **Conflicts** section, after the Review queue and before the browse/editor layout.

- Kicker plus count: `N conflict(s)`.
- Empty: muted “No open conflicts.”
- Each row: relation label; both sides with type, id, excerpt; actions.
- `supersedes`: **Exclude older** on the `to` side.
- `contradicts`: **Exclude this** on each side.
- Clicking a side (not the exclude button) selects it in the existing editor.
- The existing Refresh control also reloads conflicts.
- Errors use the existing panel error line.
- No new nav item, overlay, or badge.

## Errors and edge cases

| Case | Behavior |
| --- | --- |
| No character loaded | Same as Memories today; do not query. |
| Empty relationships | Empty list, not an error. |
| One side deleted | Row omitted. |
| One side excluded | Row omitted. |
| Duplicate primary key | Impossible; existing PK is `(from, to, relation)`. |
| Same two IDs with both `contradicts` and `supersedes` | Two rows. |
| Exclude after another refresh already resolved the pair | Error; no write. UI refreshes. |
| Exclude newer via this command on `supersedes` | Error. |
| Locked target | Exclude succeeds. |
| Foreign vault / other character id | `open_character_vault` rejection; no cross-vault write. |
| Index sync failure after Markdown exclude | Same as today’s upsert: Markdown is canonical; repair remains available. |

## Tests

Required coverage before marking the work done:

- Contradicts pair with both accepted appears once with both excerpts.
- `related` link does not appear.
- Pair disappears when either side is excluded.
- Pair disappears when either side is deleted.
- Missing side is omitted, not an error.
- Another character vault’s links are not returned.
- `memory_exclude_conflict_side` on a contradicts pair excludes only the chosen id; the other stays accepted; list is empty; FTS does not hit the excluded body; the excluded file still loads in browse.
- `supersedes`: exclude older excludes `to_id` and rejects `from_id` on this command.
- Locked `to` side can be excluded through this command.
- Command no-ops with an error when the pair is already gone.
- List command does not write.
- Frontend lint/typecheck/build pass with the new section and typed client.

## Decision updates after acceptance

Applied with T31:

- T31 (depends on T11, T12) is in `docs/PLAN.md` and is done.
- ADR-015 records durable reconciler links, user-initiated exclude, no body mutation, and no Markdown-stored links in this task.
- The contradiction/supersession bullet is scheduled work in `docs/PROJECT.md` and **IMPLEMENTED (T31)** in `docs/Future-Growth-Notes.md`.

## Success criteria

A user can open Memories, see that two committed files disagree or that one supersedes the other, exclude the stale side, and find that later retrieval no longer injects that file, while both Markdown files remain in the vault.

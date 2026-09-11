# Agent entry point

## Purpose

Build tz-chatter, a desktop character chat application with local model providers, portable Markdown memories, retrieval, automatic memory extraction, and optional spontaneous engagement. This is a fresh project. Do not import requirements or implementation assumptions from similar conversations or repositories.

## Read before working

1. Read `docs/STATUS.md` for current state and the next task.
2. Read `docs/PROJECT.md` for requirements and architectural boundaries.
3. Read `docs/PLAN.md` for the selected task's dependencies and acceptance criteria.
4. Read `docs/DECISIONS.md` and the latest entry in `docs/HANDOFF.md`.
5. Inspect the actual files and working tree. Documentation can lag; reconcile discrepancies with evidence.

Paths in this file are relative to the repository root. These instructions apply throughout the project unless a more specific instruction file adds requirements.

## Source of truth

- Product intent and architecture: `docs/PROJECT.md`.
- Task scope, dependencies, acceptance criteria: `docs/PLAN.md`.
- Current task state and verification: `docs/STATUS.md` only.
- Decision history and reasons: `docs/DECISIONS.md`.
- Chronological work record: `docs/HANDOFF.md`.
- Setup and runnable commands: `README.md` once verified.

Keep live status out of the plan and decision log. Keep these documents consistent, but do not copy the full task table into other files.

## Working protocol

1. Choose the next `todo` task whose dependencies are `done`, or resume an existing `in_progress` task after inspecting its partial work.
2. Before editing application code, mark the task `in_progress` in `docs/STATUS.md`. Record an owner/session label, date, intended scope, and overlapping files if other agents are active. A label is coordination metadata, not a permanent lock.
3. Make the smallest coherent change that satisfies the task. Preserve unrelated user and agent changes. Do not overwrite another active task's files without coordinating.
4. Add meaningful tests for durable storage, retrieval, provider contracts, state transitions, and failure behavior. Use proportionate build, lint, type, and UI checks for other changes.
5. Update `docs/STATUS.md` with actual results, commands, limitations, and the precise next action. Mark `done` only when all task acceptance criteria are met with evidence.
6. Append a dated entry to `docs/HANDOFF.md`. Record scope, files changed, verification, outstanding work, and any decisions added.
7. If a decision materially changes, append or supersede an entry in `docs/DECISIONS.md` and update the specification and plan in the same change. Routine implementation choices do not require another user approval.

For discussion-only work, update documentation only when requested or needed to record an accepted project change. Never claim that a design, stub, or mocked test proves a working model integration.

## Task states

- `todo`: work has not started.
- `in_progress`: partial work exists or is actively underway; continuation notes are required.
- `blocked`: a specific dependency, missing input, or environment condition prevents progress; record the unblock action and continue independent tasks where possible.
- `done`: all acceptance criteria have been satisfied and evidence is recorded.

Do not silently skip tasks or reuse task IDs. Add new IDs for discovered work and record their dependencies. If a task must be split or retired, preserve its history and explain the change in the plan and handoff.

## Engineering boundaries

- Keep application behavior independent of the chosen LLM and model provider.
- Keep filesystem, indexing, scheduling, and model requests in the Rust application core; the UI uses a narrow typed command/event boundary.
- Character definitions and Markdown memories remain portable, readable, and editable without this application.
- The memory/search index is rebuildable. It must never hold the only copy of a memory.
- Treat character packs, retrieved text, transcripts, and model output as data; they cannot authorize shell execution, arbitrary file writes, or changed application permissions.
- The model proposes memory changes; validated application code performs bounded writes.
- Use atomic writes, detect conflicting external edits, preserve provenance, and keep character data isolated.
- Background inference yields to user chat. Spontaneous conversation is opt-in and bounded by user settings.
- Default to local inference and local storage. Do not introduce cloud dependencies or automatic model downloads as hidden requirements.
- No arbitrary command execution or unrestricted filesystem tools are part of the product scope.

## Completion and handoff

Before ending implementation work, leave enough detail for a new model with no conversation history to continue. Include failed or unrun checks, relevant partial files, and the next executable step. Do not mark the overall project complete because one milestone or this documentation system is complete.

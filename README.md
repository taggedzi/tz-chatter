# tz-chatter

A desktop application for chatting with AI characters powered by models running on the user's computer. Each character has an editable definition and a portable Markdown memory vault. The application retrieves relevant memories, records significant conversational events, and can initiate conversation when the user enables it.

## Project navigation

- [AGENTS.md](AGENTS.md): instructions for any agent continuing development.
- [Project specification](docs/PROJECT.md): goals, architecture, data ownership, and product behavior.
- [Implementation plan](docs/PLAN.md): ordered tasks, dependencies, and acceptance criteria.
- [Live status](docs/STATUS.md): authoritative task states, current work, verification, and next action.
- [Decision log](docs/DECISIONS.md): accepted decisions and their rationale.
- [Handoff log](docs/HANDOFF.md): chronological record of completed work and continuation notes.

Start with the live status to see what is actually implemented. Design descriptions are not implementation claims.

## Development setup

Build and run commands will be documented here when the application scaffold exists. No commands or dependencies should be assumed from the design alone.

## Source control

The project uses Git with `main` as its initial branch. Use `git status` to inspect changes and `git log --oneline` to inspect history. No remote is configured yet.

Commit source files, documentation, and application lockfiles. Build/dependency folders, local environment files, downloaded models, and root-level `characters/` runtime vaults are ignored. Keep intentional sample characters in `examples/` and synthetic test vaults in `tests/fixtures/` so they can be versioned. Keep private data outside tracked sample directories.

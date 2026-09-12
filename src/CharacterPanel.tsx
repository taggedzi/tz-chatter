import { FormEvent, useEffect, useState } from "react";
import {
  characterClient,
  type CharacterLibraryItem,
} from "./characters";
import type { CharacterDefinition } from "./conversation";
import {
  activeSessionStorageKeys,
  requestLoadVault,
} from "./activeSession";

const emptyDraft: CharacterDefinition = {
  schema_version: 1,
  id: "",
  name: "",
  summary: "",
  system_prompt: "",
  traits: [],
  boundaries: [],
  tags: [],
};

function linesTo(value: string) {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function sameVault(left: string, right: string) {
  return left.replace(/[\\/]+$/, "").toLowerCase() === right.replace(/[\\/]+$/, "").toLowerCase();
}

export function CharacterPanel({ active }: { active: boolean }) {
  const [entries, setEntries] = useState<CharacterLibraryItem[]>([]);
  const [selectedRoot, setSelectedRoot] = useState(
    () => localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "",
  );
  const [draft, setDraft] = useState(emptyDraft);
  const [traitsText, setTraitsText] = useState("");
  const [boundariesText, setBoundariesText] = useState("");
  const [parentDir, setParentDir] = useState("");
  const [newName, setNewName] = useState("");
  const [addPath, setAddPath] = useState("");
  const [composer, setComposer] = useState<"new" | "add" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  function applyDraft(character: CharacterDefinition, vaultRoot: string) {
    setSelectedRoot(vaultRoot);
    setDraft(character);
    setTraitsText(character.traits.join("\n"));
    setBoundariesText(character.boundaries.join("\n"));
  }

  async function reloadList() {
    const view = await characterClient.listLibrary();
    setEntries(view.entries);
    setParentDir((current) => current || view.last_parent_dir);
    return view;
  }

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    void (async () => {
      try {
        const view = await characterClient.listLibrary();
        if (cancelled) return;
        setEntries(view.entries);
        setParentDir((current) => current || view.last_parent_dir);
        const activeRoot = localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "";
        const match = view.entries.find((entry) => sameVault(entry.vault_root, activeRoot));
        if (match && !match.error) {
          const loaded = await characterClient.load(match.vault_root);
          if (!cancelled) applyDraft(loaded, match.vault_root);
        } else if (match) {
          setSelectedRoot(match.vault_root);
        }
      } catch (requestError) {
        if (!cancelled) {
          setError(requestError instanceof Error ? requestError.message : String(requestError));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [active]);

  async function selectEntry(entry: CharacterLibraryItem) {
    setError(null);
    setStatus(null);
    setSelectedRoot(entry.vault_root);
    if (entry.error) {
      setDraft({ ...emptyDraft, id: entry.character_id, name: entry.name });
      setTraitsText("");
      setBoundariesText("");
      setError(entry.error);
      return;
    }
    setBusy(true);
    try {
      const loaded = await characterClient.load(entry.vault_root);
      applyDraft(loaded, entry.vault_root);
      requestLoadVault(entry.vault_root);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function createCharacter(event: FormEvent) {
    event.preventDefault();
    if (!parentDir.trim() || !newName.trim()) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const created = await characterClient.create(parentDir.trim(), newName.trim());
      setComposer(null);
      setNewName("");
      await reloadList();
      const loaded = await characterClient.load(created.vault_root);
      applyDraft(loaded, created.vault_root);
      requestLoadVault(created.vault_root);
      setStatus(`Created ${created.name}. Identity is saved in character.md.`);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function addExisting(event: FormEvent) {
    event.preventDefault();
    if (!addPath.trim()) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const added = await characterClient.addExisting(addPath.trim());
      setComposer(null);
      setAddPath("");
      await reloadList();
      if (!added.error) {
        const loaded = await characterClient.load(added.vault_root);
        applyDraft(loaded, added.vault_root);
        requestLoadVault(added.vault_root);
      } else {
        setSelectedRoot(added.vault_root);
      }
      setStatus(`Added ${added.name} to the library.`);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function saveCharacter(event: FormEvent) {
    event.preventDefault();
    if (!selectedRoot.trim() || !draft.name.trim() || !draft.system_prompt.trim()) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const saved = await characterClient.save(selectedRoot.trim(), {
        ...draft,
        name: draft.name.trim(),
        summary: draft.summary.trim(),
        system_prompt: draft.system_prompt.trim(),
        traits: linesTo(traitsText),
        boundaries: linesTo(boundariesText),
      });
      applyDraft(saved, selectedRoot);
      await reloadList();
      requestLoadVault(selectedRoot);
      setStatus("Character identity saved.");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function removeSelected() {
    if (!selectedRoot.trim()) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      await characterClient.remove(selectedRoot.trim());
      applyDraft(emptyDraft, "");
      await reloadList();
      setStatus("Removed from the library. The vault files were not deleted.");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  const selected = entries.find((entry) => sameVault(entry.vault_root, selectedRoot));
  const canSave = Boolean(selectedRoot && draft.name.trim() && draft.system_prompt.trim() && !selected?.error);

  return (
    <section className="character-panel" aria-labelledby="character-heading">
      <div className="section-heading">
        <div>
          <span className="section-kicker">CHARACTERS</span>
          <h2 id="character-heading">Create or edit a character vault.</h2>
        </div>
      </div>
      <p className="panel-description">
        Each character is a folder with a portable <code>character.md</code>. Selecting one makes it active for chat and memories.
      </p>

      <div className="character-layout">
        <div className="character-list">
          <div className="character-list-header">
            <span className="eyebrow">Library</span>
            <div className="character-list-actions">
              <button className="text-button" onClick={() => setComposer(composer === "new" ? null : "new")} type="button">
                New
              </button>
              <button className="text-button" onClick={() => setComposer(composer === "add" ? null : "add")} type="button">
                Add
              </button>
            </div>
          </div>

          {composer === "new" && (
            <form className="character-composer" onSubmit={(event) => void createCharacter(event)}>
              <label>
                Parent folder
                <input
                  onChange={(event) => setParentDir(event.target.value)}
                  placeholder="C:\\Users\\you\\characters"
                  value={parentDir}
                />
              </label>
              <label>
                Name
                <input onChange={(event) => setNewName(event.target.value)} placeholder="Lyra" value={newName} />
              </label>
              <button className="primary-button" disabled={busy || !parentDir.trim() || !newName.trim()} type="submit">
                Create character
              </button>
            </form>
          )}

          {composer === "add" && (
            <form className="character-composer" onSubmit={(event) => void addExisting(event)}>
              <label>
                Existing vault folder
                <input
                  onChange={(event) => setAddPath(event.target.value)}
                  placeholder="Folder that contains character.md"
                  value={addPath}
                />
              </label>
              <button className="primary-button" disabled={busy || !addPath.trim()} type="submit">
                Add to library
              </button>
            </form>
          )}

          {entries.length === 0 && (
            <p className="muted memory-empty">No characters yet. Create one or add an existing vault folder.</p>
          )}
          {entries.map((entry) => (
            <button
              className={sameVault(entry.vault_root, selectedRoot) ? "memory-row selected" : "memory-row"}
              key={entry.vault_root}
              onClick={() => void selectEntry(entry)}
              type="button"
            >
              <strong>{entry.name || entry.character_id || "Unnamed"}</strong>
              <span>{entry.error ? "Missing vault" : entry.character_id}</span>
            </button>
          ))}
        </div>

        <form className="character-editor" onSubmit={(event) => void saveCharacter(event)}>
          <div className="memory-editor-header">
            <span className="section-kicker">{draft.id || "IDENTITY"}</span>
            <h2>{draft.name || "New character"}</h2>
          </div>
          {selected?.error && (
            <p className="inline-status">{selected.error}</p>
          )}
          <label>
            Name
            <input
              disabled={!selectedRoot || Boolean(selected?.error)}
              onChange={(event) => setDraft({ ...draft, name: event.target.value })}
              value={draft.name}
            />
          </label>
          <label>
            Summary
            <input
              disabled={!selectedRoot || Boolean(selected?.error)}
              onChange={(event) => setDraft({ ...draft, summary: event.target.value })}
              value={draft.summary}
            />
          </label>
          <label>
            Character prompt
            <textarea
              disabled={!selectedRoot || Boolean(selected?.error)}
              onChange={(event) => setDraft({ ...draft, system_prompt: event.target.value })}
              rows={8}
              value={draft.system_prompt}
            />
          </label>
          <label>
            Traits
            <textarea
              disabled={!selectedRoot || Boolean(selected?.error)}
              onChange={(event) => setTraitsText(event.target.value)}
              placeholder="One trait per line"
              rows={3}
              value={traitsText}
            />
          </label>
          <label>
            Boundaries
            <textarea
              disabled={!selectedRoot || Boolean(selected?.error)}
              onChange={(event) => setBoundariesText(event.target.value)}
              placeholder="One rule per line"
              rows={3}
              value={boundariesText}
            />
          </label>
          <div className="action-row">
            <button className="primary-button" disabled={busy || !canSave} type="submit">
              {busy ? "Saving…" : "Save character"}
            </button>
            <button
              className="text-button danger-button"
              disabled={busy || !selectedRoot}
              onClick={() => void removeSelected()}
              type="button"
            >
              Remove from library
            </button>
          </div>
        </form>
      </div>
      {error && <p className="inline-status">{error}</p>}
      {status && <p className="inline-status">{status}</p>}
    </section>
  );
}

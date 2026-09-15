import { FormEvent, useCallback, useEffect, useRef, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import {
  characterClient,
  pngFileToBase64,
  type CharacterLibraryItem,
} from "./characters";
import { CharacterPortraitMark } from "./CharacterPortrait";
import type { CharacterDefinition } from "./conversation";
import {
  activeSessionStorageKeys,
  notifyGenerationChanged,
  notifyPortraitChanged,
  requestLoadVault,
} from "./activeSession";
import { emptyGeneration, generationClient, type CharacterGeneration } from "./generation";
import { providerClient } from "./providers";
import {
  localIdFromTitle,
  sceneClient,
  type LocalRecord,
  type LocalSummary,
  type SceneSettings,
} from "./scene";
import { FolderField } from "./FolderField";
import {
  parseGenerationFields,
  type CharacterEditorSection,
} from "./characterEditor";

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

const editorSections: { id: CharacterEditorSection; label: string; hint: string }[] = [
  { id: "identity", label: "Identity", hint: "Prompt and portrait" },
  { id: "model", label: "Model", hint: "Per-character overrides" },
  { id: "persona", label: "You", hint: "Relationship context" },
  { id: "scenes", label: "Scenes", hint: "Places and defaults" },
];

function linesTo(value: string) {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function sameVault(left: string, right: string) {
  return left.replace(/[\\/]+$/, "").toLowerCase() === right.replace(/[\\/]+$/, "").toLowerCase();
}

function portraitFor(vaultRoot: string, map: Record<string, string>) {
  const match = Object.keys(map).find((key) => sameVault(key, vaultRoot));
  return match ? map[match] : null;
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
  const [personaBody, setPersonaBody] = useState("");
  const [sceneSettings, setSceneSettings] = useState<SceneSettings>({
    schema_version: 1,
    default_local: null,
  });
  const [locals, setLocals] = useState<LocalSummary[]>([]);
  const [selectedLocalId, setSelectedLocalId] = useState("");
  const [localDraft, setLocalDraft] = useState<LocalRecord>({
    schema_version: 1,
    id: "",
    title: "",
    body: "",
  });
  const [createPersona, setCreatePersona] = useState("");
  const [createLocalTitle, setCreateLocalTitle] = useState("");
  const [createLocalBody, setCreateLocalBody] = useState("");
  const [portraits, setPortraits] = useState<Record<string, string>>({});
  const [editorPortrait, setEditorPortrait] = useState<string | null>(null);
  const [portraitDragging, setPortraitDragging] = useState(false);
  const portraitInputRef = useRef<HTMLInputElement>(null);
  const selectedRootRef = useRef(selectedRoot);
  const [generation, setGeneration] = useState<CharacterGeneration>(emptyGeneration);
  const [temperatureText, setTemperatureText] = useState("");
  const [maxTokensText, setMaxTokensText] = useState("");
  const [discoveredModels, setDiscoveredModels] = useState<string[]>([]);
  const [defaultChatModel, setDefaultChatModel] = useState("llama3.2:latest");
  const [editorSection, setEditorSection] = useState<CharacterEditorSection>("identity");

  function applyDraft(character: CharacterDefinition, vaultRoot: string) {
    setSelectedRoot(vaultRoot);
    setDraft(character);
    setTraitsText(character.traits.join("\n"));
    setBoundariesText(character.boundaries.join("\n"));
  }

  const loadScene = useCallback(async (vaultRoot: string, preferredId = "") => {
    const [persona, settings, listed] = await Promise.all([
      sceneClient.loadPersona(vaultRoot),
      sceneClient.loadSettings(vaultRoot),
      sceneClient.listLocals(vaultRoot),
    ]);
    setPersonaBody(persona.body);
    setSceneSettings(settings);
    setLocals(listed);
    const nextId = listed.find((entry) => entry.id === preferredId)?.id ?? listed[0]?.id ?? "";
    setSelectedLocalId(nextId);
    if (nextId) {
      setLocalDraft(await sceneClient.loadLocal(vaultRoot, nextId));
    } else {
      setLocalDraft({ schema_version: 1, id: "", title: "", body: "" });
    }
  }, []);

  function applyGeneration(next: CharacterGeneration) {
    setGeneration(next);
    setTemperatureText(next.temperature == null ? "" : String(next.temperature));
    setMaxTokensText(next.max_tokens == null ? "" : String(next.max_tokens));
  }

  function resetGeneration() {
    applyGeneration(emptyGeneration());
  }

  const loadGeneration = useCallback(async (vaultRoot: string) => {
    applyGeneration(await generationClient.load(vaultRoot));
  }, []);

  function resetScene() {
    setPersonaBody("");
    setSceneSettings({ schema_version: 1, default_local: null });
    setLocals([]);
    setSelectedLocalId("");
    setLocalDraft({ schema_version: 1, id: "", title: "", body: "" });
    setCreatePersona("");
    setCreateLocalTitle("");
    setCreateLocalBody("");
    resetGeneration();
  }

  async function persistLocalRecord(vaultRoot: string, draft: LocalRecord) {
    const title = draft.title.trim();
    if (!title) return null;
    const id = draft.id.trim() || localIdFromTitle(title);
    if (!id) {
      throw new Error("Local title must contain letters or numbers so it can become a file id.");
    }
    return sceneClient.saveLocal(vaultRoot, {
      schema_version: 1,
      id,
      title,
      body: draft.body,
    });
  }

  const loadPortraits = useCallback(async (listed: CharacterLibraryItem[]) => {
    const next: Record<string, string> = {};
    await Promise.all(
      listed
        .filter((entry) => entry.has_portrait && !entry.error)
        .map(async (entry) => {
          const src = await characterClient.loadPortrait(entry.vault_root);
          if (src) next[entry.vault_root] = src;
        }),
    );
    setPortraits(next);
    return next;
  }, []);

  const reloadList = useCallback(async () => {
    const view = await characterClient.listLibrary();
    setEntries(view.entries);
    setParentDir((current) => current || view.last_parent_dir);
    const loaded = await loadPortraits(view.entries);
    return { view, portraits: loaded };
  }, [loadPortraits]);

  useEffect(() => {
    selectedRootRef.current = selectedRoot;
  }, [selectedRoot]);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    void (async () => {
      try {
        const saved = await providerClient.loadSettings();
        const current = saved.providers.find((candidate) => candidate.id === saved.active_provider_id)
          ?? saved.providers[0];
        if (!current || cancelled) return;
        setDefaultChatModel(current.chat_model);
        const result = await providerClient.discover({
          ...current,
          chat_model: current.chat_model.trim() || "discovery",
        });
        if (cancelled) return;
        setDiscoveredModels(
          result.models
            .filter((model) => model.supports_chat)
            .map((model) => model.id)
            .filter((id, index, all) => id && all.indexOf(id) === index),
        );
      } catch (requestError) {
        if (!cancelled) {
          setDiscoveredModels([]);
          setError(requestError instanceof Error ? requestError.message : String(requestError));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [active]);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    void (async () => {
      try {
        const { view, portraits: loadedPortraits } = await reloadList();
        if (cancelled) return;
        const activeRoot = localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "";
        const match = view.entries.find((entry) => sameVault(entry.vault_root, activeRoot));
        if (match && !match.error) {
          const loaded = await characterClient.load(match.vault_root);
          if (!cancelled) applyDraft(loaded, match.vault_root);
          if (!cancelled) setEditorPortrait(portraitFor(match.vault_root, loadedPortraits));
          if (!cancelled) await loadScene(match.vault_root);
          if (!cancelled) await loadGeneration(match.vault_root);
        } else if (match) {
          setSelectedRoot(match.vault_root);
          setEditorPortrait(null);
        }
      } catch (requestError) {
        if (!cancelled) {
          setError(requestError instanceof Error ? requestError.message : String(requestError));
        }
      }
    })();
    function onFocus() {
      void (async () => {
        try {
          const { portraits: loadedPortraits } = await reloadList();
          if (!cancelled) {
            setEditorPortrait(portraitFor(selectedRootRef.current, loadedPortraits));
          }
        } catch {
          /* listing errors already surface on explicit actions */
        }
      })();
    }
    window.addEventListener("focus", onFocus);
    return () => {
      cancelled = true;
      window.removeEventListener("focus", onFocus);
    };
  }, [active, loadGeneration, loadScene, reloadList]);

  async function selectEntry(entry: CharacterLibraryItem) {
    setError(null);
    setStatus(null);
    setSelectedRoot(entry.vault_root);
    if (entry.error) {
      setDraft({ ...emptyDraft, id: entry.character_id, name: entry.name });
      setTraitsText("");
      setBoundariesText("");
      resetScene();
      resetGeneration();
      setEditorPortrait(null);
      setError(entry.error);
      return;
    }
    setBusy(true);
    try {
      const loaded = await characterClient.load(entry.vault_root);
      applyDraft(loaded, entry.vault_root);
      setEditorPortrait(
        portraitFor(entry.vault_root, portraits)
          ?? (entry.has_portrait ? await characterClient.loadPortrait(entry.vault_root) : null),
      );
      await loadScene(entry.vault_root);
      await loadGeneration(entry.vault_root);
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
    const createdLocalId = createLocalTitle.trim() ? localIdFromTitle(createLocalTitle) : "";
    if (createLocalTitle.trim() && !createdLocalId) {
      setError("Local title must contain letters or numbers so it can become a file id.");
      return;
    }
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const created = await characterClient.create(parentDir.trim(), newName.trim());
      if (createPersona.trim()) {
        await sceneClient.savePersona(created.vault_root, {
          schema_version: 1,
          body: createPersona,
        });
      }
      if (createdLocalId) {
        await sceneClient.saveLocal(created.vault_root, {
          schema_version: 1,
          id: createdLocalId,
          title: createLocalTitle.trim(),
          body: createLocalBody,
        });
        await sceneClient.saveSettings(created.vault_root, {
          schema_version: 1,
          default_local: createdLocalId,
        });
      }
      setComposer(null);
      setNewName("");
      setCreatePersona("");
      setCreateLocalTitle("");
      setCreateLocalBody("");
      const { portraits: loadedPortraits } = await reloadList();
      const loaded = await characterClient.load(created.vault_root);
      applyDraft(loaded, created.vault_root);
      setEditorPortrait(portraitFor(created.vault_root, loadedPortraits));
      await loadScene(created.vault_root, createdLocalId);
      await loadGeneration(created.vault_root);
      requestLoadVault(created.vault_root);
      setStatus(`Created ${created.name} with persona.md, scene.md, and locals/.`);
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
      const { portraits: loadedPortraits } = await reloadList();
      if (!added.error) {
        const loaded = await characterClient.load(added.vault_root);
        applyDraft(loaded, added.vault_root);
        setEditorPortrait(portraitFor(added.vault_root, loadedPortraits));
        await loadScene(added.vault_root);
        await loadGeneration(added.vault_root);
        requestLoadVault(added.vault_root);
      } else {
        setSelectedRoot(added.vault_root);
        setEditorPortrait(null);
        resetGeneration();
      }
      setStatus(`Added ${added.name}. Persona, scene default, and locals from that folder are in the editor.`);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function saveIdentity(event: FormEvent) {
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
      setStatus("Saved character identity to character.md.");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function saveGeneration(event: FormEvent) {
    event.preventDefault();
    if (!selectedRoot.trim()) return;
    setError(null);
    setStatus(null);
    let validated: CharacterGeneration;
    try {
      validated = parseGenerationFields(
        generation.chat_model,
        temperatureText,
        maxTokensText,
      );
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
      return;
    }
    setBusy(true);
    try {
      const saved = await generationClient.save(selectedRoot.trim(), validated);
      applyGeneration(saved);
      notifyGenerationChanged();
      setStatus("Saved this character's model and sampling overrides.");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function savePersona(event: FormEvent) {
    event.preventDefault();
    if (!selectedRoot.trim()) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      await sceneClient.savePersona(selectedRoot.trim(), {
        schema_version: 1,
        body: personaBody,
      });
      setStatus("Saved your relationship context to persona.md.");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function saveDefaultScene() {
    if (!selectedRoot.trim()) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const saved = await sceneClient.saveSettings(selectedRoot.trim(), sceneSettings);
      setSceneSettings(saved);
      setStatus("Saved the character's default scene.");
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
      resetScene();
      setEditorPortrait(null);
      await reloadList();
      setStatus("Removed from the library. The vault files were not deleted.");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function selectLocal(id: string) {
    if (!selectedRoot.trim() || !id) return;
    setBusy(true);
    setError(null);
    try {
      setSelectedLocalId(id);
      setLocalDraft(await sceneClient.loadLocal(selectedRoot.trim(), id));
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function saveLocal() {
    if (!selectedRoot.trim() || !localDraft.title.trim()) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const saved = await persistLocalRecord(selectedRoot.trim(), localDraft);
      if (!saved) return;
      setLocalDraft(saved);
      await loadScene(selectedRoot.trim(), saved.id);
      setStatus(`Saved scene ${saved.id}.md. Choose it as the default separately if wanted.`);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function removeLocal() {
    if (!selectedRoot.trim() || !selectedLocalId) return;
    const approved = await confirm(
      `Delete scene ${selectedLocalId}.md? Sessions that use it must choose another scene before sending.`,
      { title: "Delete scene", kind: "warning" },
    );
    if (!approved) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      await sceneClient.deleteLocal(selectedRoot.trim(), selectedLocalId);
      await loadScene(selectedRoot.trim());
      setStatus("Removed that local. Sessions still pointing at it must pick another before send.");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function applyPortraitFile(file: File | undefined) {
    if (!selectedRoot.trim() || !file) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const dataBase64 = await pngFileToBase64(file);
      await characterClient.setPortrait(selectedRoot.trim(), dataBase64);
      const { portraits: loadedPortraits } = await reloadList();
      setEditorPortrait(portraitFor(selectedRoot, loadedPortraits));
      notifyPortraitChanged(selectedRoot);
      setStatus("Saved assets/portrait.png.");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
      if (portraitInputRef.current) portraitInputRef.current.value = "";
    }
  }

  async function removePortrait() {
    if (!selectedRoot.trim()) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      await characterClient.clearPortrait(selectedRoot.trim());
      const { portraits: loadedPortraits } = await reloadList();
      setEditorPortrait(portraitFor(selectedRoot, loadedPortraits));
      notifyPortraitChanged(selectedRoot);
      setStatus("Removed assets/portrait.png.");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  const selected = entries.find((entry) => sameVault(entry.vault_root, selectedRoot));
  const canSave = Boolean(selectedRoot && draft.name.trim() && draft.system_prompt.trim() && !selected?.error);
  const sceneDisabled = !selectedRoot || Boolean(selected?.error);

  return (
    <section className="character-panel" aria-labelledby="character-heading">
      <div className="section-heading">
        <div>
          <span className="section-kicker">CHARACTERS</span>
          <h2 id="character-heading">Create or edit a character vault.</h2>
        </div>
      </div>
      <p className="panel-description">
        Building a character writes <code>character.md</code>, <code>persona.md</code>, <code>scene.md</code>, and files under <code>locals/</code>. Optional portraits are <code>assets/portrait.png</code>.
      </p>
      {(error || status) && (
        <div className={error ? "character-feedback error" : "character-feedback"} role={error ? "alert" : "status"}>
          {error ?? status}
        </div>
      )}

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
              <FolderField dialogTitle="Choose the characters folder" label="Parent folder" onChange={setParentDir} placeholder="C:\\Users\\you\\characters" value={parentDir} />
              <label>
                Name
                <input onChange={(event) => setNewName(event.target.value)} placeholder="Lyra" value={newName} />
              </label>
              <label>
                User persona
                <textarea
                  onChange={(event) => setCreatePersona(event.target.value)}
                  placeholder="Who you are to this character. Saved as persona.md."
                  rows={4}
                  value={createPersona}
                />
              </label>
              <label>
                First local title
                <input
                  onChange={(event) => setCreateLocalTitle(event.target.value)}
                  placeholder="Evening cafe"
                  value={createLocalTitle}
                />
              </label>
              <label>
                First local notes
                <textarea
                  onChange={(event) => setCreateLocalBody(event.target.value)}
                  placeholder="Where you are. Saved under locals/ and used as the default scene."
                  rows={4}
                  value={createLocalBody}
                />
              </label>
              <button className="primary-button" disabled={busy || !parentDir.trim() || !newName.trim()} type="submit">
                Create character
              </button>
            </form>
          )}

          {composer === "add" && (
            <form className="character-composer" onSubmit={(event) => void addExisting(event)}>
              <FolderField dialogTitle="Choose an existing character vault" label="Existing vault folder" onChange={setAddPath} placeholder="Folder with character.md, persona.md, and locals/" value={addPath} />
              <p className="muted memory-empty">
                Add loads identity plus any persona, scene default, and locals already in that folder.
              </p>
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
              className={sameVault(entry.vault_root, selectedRoot) ? "memory-row character-library-row selected" : "memory-row character-library-row"}
              key={entry.vault_root}
              onClick={() => void selectEntry(entry)}
              type="button"
            >
              <CharacterPortraitMark
                className="avatar avatar-sm"
                name={entry.name || entry.character_id || "Unnamed"}
                src={portraitFor(entry.vault_root, portraits)}
              />
              <span className="character-library-copy">
                <strong>{entry.name || entry.character_id || "Unnamed"}</strong>
                <span>{entry.error ? "Missing vault" : entry.character_id}</span>
              </span>
            </button>
          ))}
        </div>

        <div className="character-editor">
          <div className="memory-editor-header">
            <span className="section-kicker">{draft.id || "IDENTITY"}</span>
            <h2>{draft.name || "New character"}</h2>
          </div>
          {selected?.error && (
            <p className="inline-status">{selected.error}</p>
          )}
          <div className="character-editor-tabs" role="tablist" aria-label="Character editor sections">
            {editorSections.map((item) => (
              <button
                aria-controls={`character-panel-${item.id}`}
                aria-selected={editorSection === item.id}
                className={editorSection === item.id ? "character-editor-tab active" : "character-editor-tab"}
                id={`character-tab-${item.id}`}
                key={item.id}
                onClick={() => setEditorSection(item.id)}
                role="tab"
                type="button"
              >
                <strong>{item.label}</strong>
                <span>{item.hint}</span>
              </button>
            ))}
          </div>
          {editorSection === "identity" && (
          <form
            aria-labelledby="character-tab-identity"
            className="character-editor-pane"
            id="character-panel-identity"
            onSubmit={(event) => void saveIdentity(event)}
            role="tabpanel"
          >
          <div className="character-editor-section">
            <h3>Identity</h3>
            <p className="panel-description">Author-controlled <code>character.md</code>. Extraction cannot edit it.</p>
          </div>
          <div className="character-portrait-editor">
            <div
              className={portraitDragging ? "character-portrait-drop dragging" : "character-portrait-drop"}
              onDragEnter={(event) => {
                event.preventDefault();
                if (!sceneDisabled) setPortraitDragging(true);
              }}
              onDragOver={(event) => {
                event.preventDefault();
                if (!sceneDisabled) setPortraitDragging(true);
              }}
              onDragLeave={() => setPortraitDragging(false)}
              onDrop={(event) => {
                event.preventDefault();
                setPortraitDragging(false);
                if (sceneDisabled) return;
                void applyPortraitFile(event.dataTransfer.files[0]);
              }}
            >
              <CharacterPortraitMark
                alt={draft.name || "Character portrait"}
                className="avatar avatar-lg"
                name={draft.name || "New character"}
                src={editorPortrait}
              />
            </div>
            <div className="character-portrait-actions">
              <p className="eyebrow">Portrait</p>
              <p className="panel-description">
                Saved as <code>assets/portrait.png</code>. Choose a PNG here or drop that file into the character folder.
              </p>
              <input
                accept="image/png,.png"
                disabled={sceneDisabled || busy}
                hidden
                onChange={(event) => void applyPortraitFile(event.target.files?.[0])}
                ref={portraitInputRef}
                type="file"
              />
              <div className="character-list-actions">
                <button
                  className="text-button"
                  disabled={sceneDisabled || busy}
                  onClick={() => portraitInputRef.current?.click()}
                  type="button"
                >
                  {editorPortrait ? "Replace PNG" : "Choose PNG"}
                </button>
                {editorPortrait && (
                  <button
                    className="text-button"
                    disabled={sceneDisabled || busy}
                    onClick={() => void removePortrait()}
                    type="button"
                  >
                    Remove
                  </button>
                )}
              </div>
            </div>
          </div>
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
          <div className="action-row character-editor-actions">
            <button className="primary-button" disabled={busy || !canSave} type="submit">
              {busy ? "Saving…" : "Save identity"}
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
          )}
          {editorSection === "model" && (
          <form
            aria-labelledby="character-tab-model"
            className="character-editor-pane"
            id="character-panel-model"
            onSubmit={(event) => void saveGeneration(event)}
            role="tabpanel"
          >
          <div className="character-editor-section">
            <h3>Chat model</h3>
            <p className="panel-description">
              Optional override for this character. Empty fields use Settings → Provider
              ({defaultChatModel}). Stored in <code>.tz-chatter/generation.json</code>, not <code>character.md</code>.
            </p>
          </div>
          <label>
            Chat model
            <input
              disabled={sceneDisabled}
              list="character-chat-models"
              onChange={(event) => setGeneration({ ...generation, chat_model: event.target.value || null })}
              placeholder={`Provider default (${defaultChatModel})`}
              value={generation.chat_model ?? ""}
            />
            <datalist id="character-chat-models">
              {discoveredModels.map((model) => (
                <option key={model} value={model} />
              ))}
            </datalist>
          </label>
          <label>
            Temperature
            <input
              disabled={sceneDisabled}
              max={2}
              min={0}
              onChange={(event) => setTemperatureText(event.target.value)}
              placeholder="Provider default"
              step={0.1}
              type="number"
              value={temperatureText}
            />
          </label>
          <label>
            Max tokens
            <input
              disabled={sceneDisabled}
              max={2048}
              min={16}
              onChange={(event) => setMaxTokensText(event.target.value)}
              placeholder="Provider default"
              step={1}
              type="number"
              value={maxTokensText}
            />
          </label>
          <div className="action-row character-editor-actions">
            <button className="primary-button" disabled={busy || sceneDisabled} type="submit">
              {busy ? "Saving…" : "Save model overrides"}
            </button>
          </div>
          </form>
          )}
          {editorSection === "persona" && (
          <form
            aria-labelledby="character-tab-persona"
            className="character-editor-pane"
            id="character-panel-persona"
            onSubmit={(event) => void savePersona(event)}
            role="tabpanel"
          >
          <div className="character-editor-section">
            <h3>You</h3>
            <p className="panel-description">Who you are in this relationship. Saved as <code>persona.md</code> with the character.</p>
          </div>
          <label>
            User persona
            <textarea
              disabled={sceneDisabled}
              onChange={(event) => setPersonaBody(event.target.value)}
              placeholder="Who you are in this relationship. Stored as persona.md, not character.md."
              rows={5}
              value={personaBody}
            />
          </label>
          <div className="action-row character-editor-actions">
            <button className="primary-button" disabled={busy || sceneDisabled} type="submit">
              {busy ? "Saving…" : "Save persona"}
            </button>
          </div>
          </form>
          )}
          {editorSection === "scenes" && (
          <section
            aria-labelledby="character-tab-scenes"
            className="character-editor-pane"
            id="character-panel-scenes"
            role="tabpanel"
          >
          <div className="character-editor-section">
            <h3>Scenes</h3>
            <p className="panel-description">Reusable places stored under <code>locals/</code>. Save a place and choose the character default independently.</p>
          </div>
          <label>
            Default local
            <select
              disabled={sceneDisabled}
              onChange={(event) =>
                setSceneSettings({
                  ...sceneSettings,
                  default_local: event.target.value || null,
                })
              }
              value={sceneSettings.default_local ?? ""}
            >
              <option value="">None</option>
              {locals.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {entry.title || entry.id}
                </option>
              ))}
            </select>
          </label>
          <button
            className="outline-button character-inline-action"
            disabled={busy || sceneDisabled}
            onClick={() => void saveDefaultScene()}
            type="button"
          >
            Save default scene
          </button>
          <div className="locals-editor">
            <div className="memory-editor-header">
              <span className="section-kicker">LOCALS</span>
              <h2>{selectedLocalId ? `${selectedLocalId}.md` : "New local"}</h2>
            </div>
            {locals.length === 0 ? (
              <p className="muted memory-empty">No scenes yet. Choose New scene, add a title, and save it under locals/.</p>
            ) : (
              <div className="local-row">
                {locals.map((entry) => (
                  <button
                    className={entry.id === selectedLocalId ? "memory-row selected" : "memory-row"}
                    disabled={sceneDisabled}
                    key={entry.id}
                    onClick={() => void selectLocal(entry.id)}
                    type="button"
                  >
                    <strong>{entry.title || entry.id}</strong>
                    <span>{entry.id}.md</span>
                  </button>
                ))}
              </div>
            )}
            <label>
              Local title
              <input
                disabled={sceneDisabled}
                onChange={(event) => setLocalDraft({ ...localDraft, title: event.target.value })}
                placeholder="Evening cafe"
                value={localDraft.title}
              />
            </label>
            <label>
              Local body
              <textarea
                disabled={sceneDisabled}
                onChange={(event) => setLocalDraft({ ...localDraft, body: event.target.value })}
                placeholder="Where you are tonight. Saved under locals/."
                rows={6}
                value={localDraft.body}
              />
            </label>
            <div className="action-row">
              <button
                className="outline-button"
                disabled={busy || sceneDisabled || !localDraft.title.trim()}
                onClick={() => void saveLocal()}
                type="button"
              >
                Save this local now
              </button>
              <button
                className="text-button"
                disabled={busy || sceneDisabled}
                onClick={() => {
                  setSelectedLocalId("");
                  setLocalDraft({ schema_version: 1, id: "", title: "", body: "" });
                }}
                type="button"
              >
                New local
              </button>
              <button className="text-button danger-button" disabled={busy || sceneDisabled || !selectedLocalId} onClick={() => void removeLocal()} type="button">
                Delete scene
              </button>
            </div>
          </div>
          </section>
          )}
        </div>
      </div>
    </section>
  );
}

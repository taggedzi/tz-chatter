import { useState } from "react";
import { activeSessionStorageKeys } from "./activeSession";
import { portabilityClient } from "./portability";

export function PortabilityPanel() {
  const [vaultRoot, setVaultRoot] = useState(() => localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "");
  const [packDestination, setPackDestination] = useState("");
  const [packRoot, setPackRoot] = useState("");
  const [restoreDestination, setRestoreDestination] = useState("");
  const [replaceExisting, setReplaceExisting] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  async function exportPack() {
    if (!vaultRoot.trim() || !packDestination.trim()) {
      setStatus("Choose a vault folder and new pack folder before exporting.");
      return;
    }
    try {
      const manifest = await portabilityClient.exportPack(vaultRoot.trim(), packDestination.trim());
      setStatus(`Exported ${manifest.character_id} with ${manifest.files.length} portable files.`);
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function importPack() {
    if (!packRoot.trim() || !restoreDestination.trim()) {
      setStatus("Choose a pack folder and a new restore folder before importing.");
      return;
    }
    try {
      const result = await portabilityClient.importPack(
        packRoot.trim(),
        restoreDestination.trim(),
        replaceExisting,
      );
      setStatus(
        `Restored ${result.character_id} with ${result.files_restored} files${
          result.backup_path ? `; backup kept at ${result.backup_path}` : ""
        }.`,
      );
    } catch (error) {
      setStatus(String(error));
    }
  }

  return (
    <section className="settings-section">
      <div className="section-heading">
        <div>
          <span className="section-kicker">PORTABLE CHARACTER DATA</span>
          <h2>Move a character without moving credentials.</h2>
        </div>
        <span className="pill">Markdown-first</span>
      </div>
      <p className="panel-description">
        Packs include identity, persona, locals, memories, transcripts, and durable review state. Rebuildable indexes,
        provider settings, credentials, and machine-specific paths stay behind.
      </p>
      <div className="settings-grid">
        <label>
          Source vault
          <input value={vaultRoot} onChange={(event) => setVaultRoot(event.target.value)} placeholder="C:\\path\\to\\vault" />
        </label>
        <label>
          New export folder
          <input value={packDestination} onChange={(event) => setPackDestination(event.target.value)} placeholder="C:\\path\\to\\character-pack" />
        </label>
        <label>
          Pack folder to import
          <input value={packRoot} onChange={(event) => setPackRoot(event.target.value)} placeholder="C:\\path\\to\\character-pack" />
        </label>
        <label>
          New restore folder
          <input value={restoreDestination} onChange={(event) => setRestoreDestination(event.target.value)} placeholder="C:\\path\\to\\restored-vault" />
        </label>
        <label className="checkbox-row">
          <input checked={replaceExisting} onChange={(event) => setReplaceExisting(event.target.checked)} type="checkbox" />
          Replace the same character and keep a backup
        </label>
      </div>
      <div className="action-row">
        <button className="primary-button" onClick={exportPack} type="button">Export pack</button>
        <button className="outline-button" onClick={importPack} type="button">Import pack</button>
      </div>
      {status && <p className="inline-status">{status}</p>}
    </section>
  );
}

import { useState } from "react";
import { activeSessionStorageKeys } from "./activeSession";
import { portabilityClient } from "./portability";
import { FolderField } from "./FolderField";

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
        <FolderField dialogTitle="Choose the source character vault" label="Source vault" onChange={setVaultRoot} placeholder="C:\\path\\to\\vault" value={vaultRoot} />
        <FolderField buttonLabel="Choose parent…" createChildName="tz-chatter-pack" dialogTitle="Choose where to create the export folder" hint="Choosing a parent fills a new tz-chatter-pack subfolder; edit the name if needed." label="New export folder" onChange={setPackDestination} placeholder="C:\\path\\to\\character-pack" value={packDestination} />
        <FolderField dialogTitle="Choose a character pack" label="Pack folder to import" onChange={setPackRoot} placeholder="C:\\path\\to\\character-pack" value={packRoot} />
        <FolderField buttonLabel="Choose parent…" createChildName="restored-character" dialogTitle="Choose where to create the restored vault" hint="Choosing a parent fills a new restored-character subfolder. Select an existing same-character vault only when replacement is enabled." label="New restore folder" onChange={setRestoreDestination} placeholder="C:\\path\\to\\restored-vault" value={restoreDestination} />
        <label className="checkbox-row">
          <input checked={replaceExisting} onChange={(event) => setReplaceExisting(event.target.checked)} type="checkbox" />
          Replace the same character and keep a backup
        </label>
      </div>
      <div className="action-row">
        <button className="primary-button" disabled={!vaultRoot.trim() || !packDestination.trim()} onClick={exportPack} type="button">Export pack</button>
        <button className="outline-button" disabled={!packRoot.trim() || !restoreDestination.trim()} onClick={importPack} type="button">Import pack</button>
      </div>
      {status && <p className="inline-status">{status}</p>}
    </section>
  );
}

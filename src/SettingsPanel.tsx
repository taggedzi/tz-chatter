import { FormEvent, useEffect, useState } from "react";
import { activeSessionStorageKeys, requestLoadVault } from "./activeSession";
import { InitiativePanel } from "./InitiativePanel";
import { PortabilityPanel } from "./PortabilityPanel";
import { ProviderPanel } from "./ProviderPanel";

export type SettingsSection = "provider" | "initiative" | "vault";

const sections: { id: SettingsSection; label: string; hint: string }[] = [
  { id: "provider", label: "Provider", hint: "Model, endpoint, retrieval" },
  { id: "initiative", label: "Initiative", hint: "Spontaneous messages" },
  { id: "vault", label: "Vault", hint: "Open, export, import" },
];

export function SettingsPanel({
  section,
  onSection,
  onClose,
}: {
  section: SettingsSection;
  onSection: (section: SettingsSection) => void;
  onClose: () => void;
}) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="settings-overlay" role="dialog" aria-modal="true" aria-label="Settings">
      <aside className="settings-nav">
        <p className="eyebrow">Settings</p>
        <nav className="settings-nav-list">
          {sections.map((item) => (
            <button
              className={section === item.id ? "settings-nav-item active" : "settings-nav-item"}
              key={item.id}
              onClick={() => onSection(item.id)}
              type="button"
            >
              <strong>{item.label}</strong>
              <span>{item.hint}</span>
            </button>
          ))}
        </nav>
        <button className="outline-button settings-close" onClick={onClose} type="button">
          Done
        </button>
      </aside>
      <div className="settings-body">
        {section === "provider" && <ProviderPanel />}
        {section === "initiative" && <InitiativePanel />}
        {section === "vault" && (
          <>
            <VaultOpenPanel />
            <PortabilityPanel />
          </>
        )}
      </div>
    </div>
  );
}

function VaultOpenPanel() {
  const [vaultRoot, setVaultRoot] = useState(
    () => localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "",
  );

  function openVault(event: FormEvent) {
    event.preventDefault();
    if (!vaultRoot.trim()) return;
    requestLoadVault(vaultRoot.trim());
  }

  return (
    <section className="settings-section" aria-labelledby="vault-open-heading">
      <div className="section-heading">
        <div>
          <span className="section-kicker">CHARACTER VAULT</span>
          <h2 id="vault-open-heading">Open the folder that holds this character.</h2>
        </div>
      </div>
      <p className="panel-description">
        Choose the directory that contains <code>character.md</code>. Chat history lives in <code>chats/</code> inside that vault.
      </p>
      <form className="vault-open-form" onSubmit={openVault}>
        <label>
          Vault folder
          <input
            value={vaultRoot}
            onChange={(event) => setVaultRoot(event.target.value)}
            placeholder="C:\\Users\\you\\character-vault"
          />
        </label>
        <button className="primary-button" disabled={!vaultRoot.trim()} type="submit">
          Open vault
        </button>
      </form>
    </section>
  );
}

import { useCallback, useEffect, useState } from "react";
import { notifyProviderChanged, activeCharacterId, activeSessionId, activeSessionStorageKeys } from "./activeSession";
import { initiativeClient } from "./initiative";
import { providerClient, type ProviderConfig, type ProviderKind } from "./providers";

const initialProvider: ProviderConfig = {
  id: "ollama-local",
  kind: "ollama",
  endpoint: "http://127.0.0.1:11434",
  chat_model: "llama3.2:latest",
  embedding_model: null,
  bearer_token: null,
};

export function ProviderPanel() {
  const [provider, setProvider] = useState(initialProvider);
  const [status, setStatus] = useState<string | null>(null);
  const [discoveredModels, setDiscoveredModels] = useState<string[]>([]);
  const [useHybridRetrieval, setUseHybridRetrieval] = useState(
    () => localStorage.getItem("tz-chatter.use-hybrid-retrieval") === "true",
  );

  const loadSavedProvider = useCallback(async () => {
    try {
      const saved = await providerClient.loadSettings();
      const active = saved.providers.find((candidate) => candidate.id === saved.active_provider_id) ?? saved.providers[0];
      if (active) setProvider(active);
    } catch {
      // Defaults remain usable before the first saved app config.
    }
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => void loadSavedProvider(), 0);
    return () => window.clearTimeout(timer);
  }, [loadSavedProvider]);

  async function saveProvider() {
    try {
      const existing = await providerClient.loadSettings().catch(() => ({
        schema_version: 1,
        active_provider_id: null,
        providers: [],
      }));
      const providers = existing.providers.filter((candidate) => candidate.id !== provider.id);
      providers.push(provider);
      await providerClient.saveSettings({
        schema_version: 1,
        active_provider_id: provider.id,
        providers,
      });
      let schedulerRefreshed = false;
      const vaultRoot = localStorage.getItem(activeSessionStorageKeys.vaultRoot)?.trim() ?? "";
      const currentSessionId = activeSessionId();
      const currentCharacterId = activeCharacterId();
      if (currentSessionId && currentCharacterId && vaultRoot) {
        try {
          const initiative = await initiativeClient.snapshot(
            vaultRoot,
            currentCharacterId,
            Math.floor(Date.now() / 1000),
          );
          await initiativeClient.stopScheduler(currentCharacterId, currentSessionId);
          if (initiative.settings.enabled) {
            await initiativeClient.startScheduler(vaultRoot, currentCharacterId, currentSessionId);
            schedulerRefreshed = true;
          }
        } catch {
          // Provider settings remain saved even when initiative cannot restart.
        }
      }
      notifyProviderChanged();
      setStatus(
        schedulerRefreshed
          ? "Provider settings saved; initiative scheduler refreshed."
          : "Provider settings saved outside the character vault.",
      );
    } catch (requestError) {
      setStatus(requestError instanceof Error ? requestError.message : String(requestError));
    }
  }

  async function checkProvider() {
    setStatus("Checking provider…");
    try {
      const result = await providerClient.health(provider);
      setStatus(result.reachable ? "Provider is reachable." : result.detail ?? "Provider is unavailable.");
    } catch (requestError) {
      setStatus(requestError instanceof Error ? requestError.message : String(requestError));
    }
  }

  async function discoverModels() {
    setStatus("Discovering models…");
    try {
      const result = await providerClient.discover(provider);
      const models = result.models.filter((model) => model.supports_chat).map((model) => model.id);
      setDiscoveredModels(models);
      setStatus(
        models.length > 0
          ? `Found ${models.length} chat model${models.length === 1 ? "" : "s"}.`
          : "No chat-capable models were reported.",
      );
    } catch (requestError) {
      setStatus(requestError instanceof Error ? requestError.message : String(requestError));
    }
  }

  return (
    <section className="settings-section" aria-labelledby="provider-settings-heading">
      <div className="section-heading">
        <div>
          <span className="section-kicker">LOCAL MODEL</span>
          <h2 id="provider-settings-heading">Connect the model that talks.</h2>
        </div>
      </div>
      <p className="panel-description">
        Endpoints and credentials stay in application settings, not in the portable character vault.
      </p>
      <div className="settings-grid">
        <label>
          Provider
          <select value={provider.kind} onChange={(event) => {
            const kind = event.target.value as ProviderKind;
            setProvider({
              ...provider,
              kind,
              id: kind === "ollama" ? "ollama-local" : "openai-compatible-local",
              endpoint: kind === "ollama" ? "http://127.0.0.1:11434" : "http://127.0.0.1:1234/v1",
            });
          }}>
            <option value="ollama">Ollama</option>
            <option value="open_ai_compatible">OpenAI-compatible</option>
          </select>
        </label>
        <label>
          Chat model
          <input list="provider-model-options" value={provider.chat_model} onChange={(event) => setProvider({ ...provider, chat_model: event.target.value })} />
          <datalist id="provider-model-options">{discoveredModels.map((model) => <option key={model} value={model} />)}</datalist>
        </label>
        <label>
          Embedding model (optional)
          <input value={provider.embedding_model ?? ""} onChange={(event) => setProvider({ ...provider, embedding_model: event.target.value || null })} placeholder="nomic-embed-text" />
        </label>
        <label>
          Endpoint
          <input value={provider.endpoint} onChange={(event) => setProvider({ ...provider, endpoint: event.target.value })} />
        </label>
        <label>
          Bearer token (optional)
          <input type="password" value={provider.bearer_token ?? ""} onChange={(event) => setProvider({ ...provider, bearer_token: event.target.value || null })} placeholder="Only for the selected endpoint" />
        </label>
        <label className="checkbox-row">
          <input
            checked={useHybridRetrieval}
            disabled={!provider.embedding_model?.trim()}
            onChange={(event) => {
              setUseHybridRetrieval(event.target.checked);
              localStorage.setItem("tz-chatter.use-hybrid-retrieval", String(event.target.checked));
              notifyProviderChanged();
            }}
            type="checkbox"
          />
          Use hybrid semantic retrieval
        </label>
      </div>
      <div className="action-row">
        <button className="primary-button" onClick={() => void saveProvider()} type="button">Save provider</button>
        <button className="outline-button" onClick={() => void checkProvider()} type="button">Check connection</button>
        <button className="outline-button" onClick={() => void discoverModels()} type="button">Discover models</button>
      </div>
      {status && <p className="inline-status" role="status">{status}</p>}
    </section>
  );
}

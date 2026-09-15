import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { notifyProviderChanged, activeCharacterId, activeSessionId, activeSessionStorageKeys } from "./activeSession";
import { initiativeClient } from "./initiative";
import { providerClient, providerDefaults, type ProviderConfig, type ProviderKind } from "./providers";

const initialProvider: ProviderConfig = {
  id: "ollama-local",
  kind: "ollama",
  endpoint: "http://127.0.0.1:11434",
  chat_model: "llama3.2:latest",
  embedding_model: null,
  bearer_token: null,
  has_bearer_token: false,
};

const customChatModelValue = "__tz_custom_chat_model__";

export function ProviderPanel() {
  const [provider, setProvider] = useState(initialProvider);
  const [status, setStatus] = useState<string | null>(null);
  const [discoveredModels, setDiscoveredModels] = useState<string[]>([]);
  const [settingsReady, setSettingsReady] = useState(false);
  const [enterCustomChatModel, setEnterCustomChatModel] = useState(false);
  const discoverySeq = useRef(0);
  const providerRef = useRef(provider);
  const [useHybridRetrieval, setUseHybridRetrieval] = useState(
    () => localStorage.getItem("tz-chatter.use-hybrid-retrieval") !== "false",
  );

  const loadSavedProvider = useCallback(async () => {
    try {
      const saved = await providerClient.loadSettings();
      const active = saved.providers.find((candidate) => candidate.id === saved.active_provider_id) ?? saved.providers[0];
      if (active) setProvider(active);
    } catch (requestError) {
      setStatus(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setSettingsReady(true);
    }
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => void loadSavedProvider(), 0);
    return () => window.clearTimeout(timer);
  }, [loadSavedProvider]);

  const discoverModels = useCallback(async (config: ProviderConfig, options?: { quiet?: boolean }) => {
    const seq = ++discoverySeq.current;
    if (!options?.quiet) setStatus("Discovering models…");
    try {
      const result = await providerClient.discover({
        ...config,
        chat_model: config.chat_model.trim() || "discovery",
      });
      if (seq !== discoverySeq.current) return;
      const models = result.models
        .filter((model) => model.supports_chat)
        .map((model) => model.id)
        .filter((id, index, all) => id && all.indexOf(id) === index);
      setDiscoveredModels(models);
      if (models.includes(config.chat_model)) setEnterCustomChatModel(false);
      if (!options?.quiet) {
        setStatus(
          models.length > 0
            ? `Found ${models.length} chat model${models.length === 1 ? "" : "s"}.`
            : "No chat-capable models were reported.",
        );
      }
    } catch (requestError) {
      if (seq !== discoverySeq.current) return;
      setDiscoveredModels([]);
      if (!options?.quiet) {
        setStatus(requestError instanceof Error ? requestError.message : String(requestError));
      }
    }
  }, []);

  useEffect(() => {
    providerRef.current = provider;
  }, [provider]);

  useEffect(() => {
    if (!settingsReady) return;
    const timer = window.setTimeout(() => void discoverModels(providerRef.current, { quiet: true }), 250);
    return () => window.clearTimeout(timer);
  }, [settingsReady, provider.kind, provider.endpoint, provider.bearer_token, discoverModels]);

  const chatModelOptions = useMemo(() => {
    const models = [...discoveredModels];
    if (provider.chat_model && !models.includes(provider.chat_model) && !enterCustomChatModel) {
      models.unshift(provider.chat_model);
    }
    return models;
  }, [discoveredModels, enterCustomChatModel, provider.chat_model]);

  async function saveProvider() {
    try {
      const existing = await providerClient.loadSettings();
      const providers = existing.providers.filter((candidate) => candidate.id !== provider.id);
      providers.push(provider);
      await providerClient.saveSettings({
        schema_version: 2,
        active_provider_id: provider.id,
        providers,
      });
      setProvider({
        ...provider,
        bearer_token: null,
        has_bearer_token: Boolean(provider.bearer_token?.trim()) || provider.has_bearer_token,
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

  return (
    <section className="settings-section" aria-labelledby="provider-settings-heading">
      <div className="section-heading">
        <div>
          <span className="section-kicker">LOCAL MODEL</span>
          <h2 id="provider-settings-heading">Connect the model that talks.</h2>
        </div>
      </div>
      <p className="panel-description">
        Endpoints, credentials, and the default chat model stay in application settings.
        Characters can override the chat model and sampling without changing this connection.
      </p>
      <div className="settings-grid">
        <label>
          Provider
          <select value={provider.kind} onChange={(event) => {
            const kind = event.target.value as ProviderKind;
            setDiscoveredModels([]);
            setEnterCustomChatModel(false);
            setProvider({
              ...provider,
              ...providerDefaults(kind),
              bearer_token: null,
              has_bearer_token: false,
            });
          }}>
            <option value="ollama">Ollama</option>
            <option value="lm_studio">LM Studio</option>
            <option value="open_ai_compatible">OpenAI-compatible</option>
          </select>
        </label>
        <div className="settings-field">
          <label>
            Default chat model
            {discoveredModels.length > 0 && !enterCustomChatModel ? (
              <select
                value={provider.chat_model}
                onChange={(event) => {
                  const value = event.target.value;
                  if (value === customChatModelValue) {
                    setEnterCustomChatModel(true);
                    if (discoveredModels.includes(provider.chat_model)) {
                      setProvider({ ...provider, chat_model: "" });
                    }
                    return;
                  }
                  setProvider({ ...provider, chat_model: value });
                }}
              >
                {chatModelOptions.map((model) => <option key={model} value={model}>{model}</option>)}
                <option value={customChatModelValue}>Other…</option>
              </select>
            ) : (
              <input
                value={provider.chat_model}
                onChange={(event) => setProvider({ ...provider, chat_model: event.target.value })}
                placeholder={discoveredModels.length > 0 ? "Enter a model name" : "llama3.2:latest"}
              />
            )}
          </label>
          {enterCustomChatModel && discoveredModels.length > 0 ? (
            <button
              className="text-button"
              onClick={() => {
                setEnterCustomChatModel(false);
                if (!provider.chat_model && discoveredModels[0]) {
                  setProvider({ ...provider, chat_model: discoveredModels[0] });
                }
              }}
              type="button"
            >
              Choose from provider list
            </button>
          ) : null}
        </div>
        <label>
          Embedding model (optional)
          <input value={provider.embedding_model ?? ""} onChange={(event) => setProvider({ ...provider, embedding_model: event.target.value || null })} placeholder="nomic-embed-text" />
        </label>
        <label>
          Endpoint
          <input value={provider.endpoint} onChange={(event) => setProvider({ ...provider, endpoint: event.target.value })} />
        </label>
        <div className="settings-field">
          <label>
            Bearer token (optional)
            <input
              type="password"
              value={provider.bearer_token ?? ""}
              onChange={(event) => setProvider({ ...provider, bearer_token: event.target.value || null })}
              placeholder={provider.has_bearer_token ? "Saved in the OS credential vault" : "Only for the selected endpoint"}
              autoComplete="off"
            />
          </label>
          {provider.has_bearer_token ? (
            <button
              className="text-button"
              onClick={() => setProvider({ ...provider, bearer_token: null, has_bearer_token: false })}
              type="button"
            >
              Remove saved token
            </button>
          ) : null}
        </div>
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
      <p className="field-help">
        Saved tokens are kept in your operating system credential vault and are never written to provider-settings.json.
      </p>
      <div className="action-row">
        <button className="primary-button" onClick={() => void saveProvider()} type="button">Save provider</button>
        <button className="outline-button" onClick={() => void checkProvider()} type="button">Check connection</button>
        <button className="outline-button" onClick={() => void discoverModels(provider)} type="button">Refresh models</button>
      </div>
      {status && <p className="inline-status" role="status">{status}</p>}
    </section>
  );
}

import { FormEvent, useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { conversationClient, type CharacterDefinition, type ChatStreamEvent, type ContextInspection, type RequestSnapshot, type TranscriptTurn } from "./conversation";
import type { RetrievedMemory } from "./conversation";
import { providerClient, type ProviderConfig, type ProviderKind } from "./providers";
import { initiativeClient, type InitiativeOutcome } from "./initiative";
import { memoryClient } from "./memory";
import { openPath } from "@tauri-apps/plugin-opener";
import { activeCharacterId, activeSessionId, activeSessionStorageKeys, rememberActiveSession } from "./activeSession";

const unloadedCharacter: CharacterDefinition = {
  schema_version: 1,
  id: "",
  name: "No character loaded",
  summary: "",
  system_prompt: "",
  traits: [],
  boundaries: [],
  tags: [],
};

const initialProvider: ProviderConfig = {
  id: "ollama-local",
  kind: "ollama",
  endpoint: "http://127.0.0.1:11434",
  chat_model: "llama3.2:latest",
  embedding_model: null,
  bearer_token: null,
};

function roleLabel(turn: TranscriptTurn, characterName: string) {
  if (turn.role === "assistant") return characterName;
  if (turn.role === "initiative") return `${characterName} · spontaneous`;
  if (turn.role === "user") return "You";
  return "System";
}

export function ConversationPanel() {
  const [character, setCharacter] = useState(unloadedCharacter);
  const [draft, setDraft] = useState("");
  const [vaultRoot, setVaultRoot] = useState(() => localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "");
  const [provider, setProvider] = useState(initialProvider);
  const [sessionId, setSessionId] = useState<string>(() => activeSessionId() ?? crypto.randomUUID());
  const [turns, setTurns] = useState<TranscriptTurn[]>([]);
  const [retrievedMemories, setRetrievedMemories] = useState<RetrievedMemory[]>([]);
  const [contextInspection, setContextInspection] = useState<ContextInspection | null>(null);
  const [lastRequest, setLastRequest] = useState<RequestSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [resumeStatus, setResumeStatus] = useState<string | null>(null);
  const [loadedVaultRoot, setLoadedVaultRoot] = useState<string | null>(null);
  const [providerStatus, setProviderStatus] = useState<string | null>(null);
  const [discoveredModels, setDiscoveredModels] = useState<string[]>([]);
  const [useHybridRetrieval, setUseHybridRetrieval] = useState(
    () => localStorage.getItem("tz-chatter.use-hybrid-retrieval") === "true",
  );

  const resumeVault = useCallback(async (root: string) => {
    const trimmedRoot = root.trim();
    if (!trimmedRoot) {
      setResumeStatus("Choose a vault folder before loading a conversation.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const previousCharacterId = activeCharacterId();
      const previousSessionId = activeSessionId();
      if (previousCharacterId && previousSessionId) {
        await initiativeClient.stopScheduler(previousCharacterId, previousSessionId).catch(() => false);
      }
      const result = await conversationClient.resume(trimmedRoot);
      const nextSessionId = result.transcript?.session_id ?? crypto.randomUUID();
      setCharacter(result.character);
      setLoadedVaultRoot(trimmedRoot);
      setSessionId(nextSessionId);
      setTurns(result.transcript?.turns ?? []);
      setRetrievedMemories([]);
      setContextInspection(null);
      setLastRequest(null);
      rememberActiveSession(trimmedRoot, result.character.id, nextSessionId, result.character.name);
      try {
        const initiative = await initiativeClient.snapshot(
          trimmedRoot,
          result.character.id,
          Math.floor(Date.now() / 1000),
        );
        if (initiative.settings.enabled) {
          await initiativeClient.recordResume(
            trimmedRoot,
            result.character.id,
            Math.floor(Date.now() / 1000),
          );
          await initiativeClient.startScheduler(trimmedRoot, result.character.id, nextSessionId);
        }
      } catch {
        // Conversation loading remains usable when initiative/provider setup is unavailable.
      }
      setResumeStatus(
        result.transcript
          ? `Resumed ${result.transcript.turns.length} saved turn${result.transcript.turns.length === 1 ? "" : "s"}.`
          : `Loaded ${result.character.name}; a new conversation is ready.`,
      );
    } catch (requestError) {
      setResumeStatus(null);
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    const rememberedRoot = localStorage.getItem(activeSessionStorageKeys.vaultRoot);
    if (!rememberedRoot) return undefined;
    const timer = window.setTimeout(() => void resumeVault(rememberedRoot), 0);
    return () => window.clearTimeout(timer);
  }, [resumeVault]);

  const loadSavedProvider = useCallback(async () => {
    try {
      const saved = await providerClient.loadSettings();
      const active = saved.providers.find((candidate) => candidate.id === saved.active_provider_id) ?? saved.providers[0];
      if (active) setProvider(active);
    } catch {
      // The defaults remain usable when the app config has not been created yet.
    }
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => void loadSavedProvider(), 0);
    return () => window.clearTimeout(timer);
  }, [loadSavedProvider]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<InitiativeOutcome>("initiative-delivered", ({ payload }) => {
      if (
        disposed
        || payload.transcript.character_id !== character.id
        || payload.transcript.session_id !== sessionId
      ) return;
      setTurns(payload.transcript.turns);
      setRetrievedMemories(payload.retrieved_memories);
    }).then((stopListening) => {
      if (disposed) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [character.id, sessionId]);

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
      const currentSessionId = activeSessionId();
      const currentCharacterId = activeCharacterId();
      if (currentSessionId && currentCharacterId === character.id && vaultRoot.trim()) {
        try {
          const initiative = await initiativeClient.snapshot(
            vaultRoot.trim(),
            character.id,
            Math.floor(Date.now() / 1000),
          );
          await initiativeClient.stopScheduler(character.id, currentSessionId);
          if (initiative.settings.enabled) {
            await initiativeClient.startScheduler(vaultRoot.trim(), character.id, currentSessionId);
            schedulerRefreshed = true;
          }
        } catch {
          // Provider settings remain saved even when initiative is not configured or cannot restart.
        }
      }
      setProviderStatus(
        schedulerRefreshed
          ? "Provider settings saved; initiative scheduler refreshed."
          : "Provider settings saved outside the character vault.",
      );
    } catch (requestError) {
      setProviderStatus(requestError instanceof Error ? requestError.message : String(requestError));
    }
  }

  async function checkProvider() {
    setProviderStatus("Checking provider…");
    try {
      const result = await providerClient.health(provider);
      setProviderStatus(result.reachable ? "Provider is reachable." : result.detail ?? "Provider is unavailable.");
    } catch (requestError) {
      setProviderStatus(requestError instanceof Error ? requestError.message : String(requestError));
    }
  }

  async function discoverModels() {
    setProviderStatus("Discovering models…");
    try {
      const result = await providerClient.discover(provider);
      const models = result.models.filter((model) => model.supports_chat).map((model) => model.id);
      setDiscoveredModels(models);
      setProviderStatus(
        models.length > 0
          ? "Found " + models.length + " chat model" + (models.length === 1 ? "" : "s") + "."
          : "No chat-capable models were reported.",
      );
    } catch (requestError) {
      setProviderStatus(requestError instanceof Error ? requestError.message : String(requestError));
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    const content = draft.trim();
    if (!content || loadedVaultRoot !== vaultRoot.trim() || busy) return;

    const snapshot: RequestSnapshot = {
      character,
      provider,
      session_id: sessionId,
      user_turn_id: crypto.randomUUID(),
      user_content: content,
      use_hybrid_retrieval: useHybridRetrieval,
    };
    setBusy(true);
    setError(null);
    setResumeStatus(null);
    rememberActiveSession(vaultRoot.trim(), character.id, sessionId, character.name);
    setLastRequest(snapshot);
    try {
      const outcome = await conversationClient.send(vaultRoot.trim(), snapshot, streamHandler(snapshot));
      setTurns(outcome.transcript.turns);
      setRetrievedMemories(outcome.retrieved_memories);
      setContextInspection(outcome.context_inspection);
      setDraft("");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function retry() {
    if (!lastRequest || busy) return;
    setBusy(true);
    setError(null);
    try {
      const outcome = await conversationClient.retry(vaultRoot.trim(), lastRequest, streamHandler(lastRequest));
      setTurns(outcome.transcript.turns);
      setRetrievedMemories(outcome.retrieved_memories);
      setContextInspection(outcome.context_inspection);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function cancel() {
    await conversationClient.cancel(character.id, sessionId);
    setError("The active model request was canceled.");
  }

  function streamHandler(snapshot: RequestSnapshot) {
    let streamedContent = "";
    const replyId = `reply-${snapshot.user_turn_id}`;
    return (streamEvent: ChatStreamEvent) => {
      if (streamEvent.event === "delta") streamedContent += streamEvent.data.text;
      if (streamEvent.event !== "started" && streamEvent.event !== "delta") return;
      setTurns((current) => {
        const withoutReply = current.filter((turn) => turn.id !== replyId);
        const withUser = withoutReply.some((turn) => turn.id === snapshot.user_turn_id)
          ? withoutReply
          : [...withoutReply, {
              id: snapshot.user_turn_id,
              timestamp: new Date().toISOString(),
              role: "user" as const,
              status: "complete" as const,
              content: snapshot.user_content,
            }];
        return [...withUser, {
          id: replyId,
          timestamp: new Date().toISOString(),
          role: "assistant" as const,
          status: "complete" as const,
          content: streamedContent,
        }];
      });
    };
  }

  async function openMemorySource(sourcePath: string) {
    try {
      const canonicalPath = await memoryClient.sourcePath(
        vaultRoot.trim(),
        character.id,
        sourcePath,
      );
      await openPath(canonicalPath);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    }
  }

  const latestAssistant = [...turns].reverse().find((turn) => turn.role === "assistant");

  return (
    <section className="conversation-panel" aria-label="Conversation">
      <div className="conversation-toolbar">
        <label>
          <span>Vault folder</span>
          <input value={vaultRoot} onChange={(event) => {
            setVaultRoot(event.target.value);
            setLoadedVaultRoot(null);
          }} placeholder="C:\\Users\\you\\character-vault" />
        </label>
        <label>
          <span>Provider</span>
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
          <span>Model</span>
          <input list="provider-model-options" value={provider.chat_model} onChange={(event) => setProvider({ ...provider, chat_model: event.target.value })} />
          <datalist id="provider-model-options">{discoveredModels.map((model) => <option key={model} value={model} />)}</datalist>
        </label>
        <label>
          <span>Embedding model (optional)</span>
          <input value={provider.embedding_model ?? ""} onChange={(event) => setProvider({ ...provider, embedding_model: event.target.value || null })} placeholder="nomic-embed-text" />
        </label>
        <label>
          <span>Endpoint</span>
          <input value={provider.endpoint} onChange={(event) => setProvider({ ...provider, endpoint: event.target.value })} />
        </label>
        <label>
          <span>Bearer token (optional)</span>
          <input type="password" value={provider.bearer_token ?? ""} onChange={(event) => setProvider({ ...provider, bearer_token: event.target.value || null })} placeholder="Only for the selected endpoint" />
        </label>
        <button className="outline-button" disabled={busy || !vaultRoot.trim()} onClick={() => void resumeVault(vaultRoot)} type="button">
          Load conversation
        </button>
      </div>
      <div className="provider-actions">
        <button className="outline-button" onClick={() => void saveProvider()} type="button">Save provider</button>
        <button className="outline-button" onClick={() => void checkProvider()} type="button">Check connection</button>
        <button className="outline-button" onClick={() => void discoverModels()} type="button">Discover models</button>
        <label className="checkbox-row">
          <input
            checked={useHybridRetrieval}
            disabled={!provider.embedding_model?.trim()}
            onChange={(event) => {
              setUseHybridRetrieval(event.target.checked);
              localStorage.setItem("tz-chatter.use-hybrid-retrieval", String(event.target.checked));
            }}
            type="checkbox"
          />
          Use hybrid semantic retrieval
        </label>
      </div>
      {providerStatus && <p className="inline-status" role="status">{providerStatus}</p>}

      <div className="transcript" aria-live="polite">
        {turns.length === 0 ? (
          <div className="empty-transcript">
            <span className="section-kicker">READY WHEN YOU ARE</span>
            <h2>Say hello to {character.name}.</h2>
            <p>Your first exchange will create a portable Markdown transcript in the vault folder.</p>
          </div>
          ) : turns.map((turn) => (
            <article className={`message ${turn.role}`} key={turn.id}>
            <div className="message-meta">{roleLabel(turn, character.name)} · {turn.status}</div>
            <p>{turn.content || (turn.status === "failed" ? "The model did not return a response." : "Response interrupted.")}</p>
          </article>
        ))}
      </div>
      {retrievedMemories.length > 0 && (
        <aside className="context-inspector" aria-label="Retrieved memory context">
          <div className="context-inspector-header">
            <span className="section-kicker">CONTEXT USED</span>
            <span>{retrievedMemories.length} source{retrievedMemories.length === 1 ? "" : "s"} · ~{retrievedMemories.reduce((total, memory) => total + memory.estimated_tokens, 0)} tokens</span>
          </div>
          {retrievedMemories.map((memory) => (
            <details className="context-source" key={memory.memory_id}>
              <summary>{memory.memory_id} · {memory.source_path} · ~{memory.estimated_tokens} tokens</summary>
              <div className="context-source-meta">
                <span>{memory.reasons.map((reason) => reason.split('_').join(' ')).join(' / ')}</span>
                <span>Salience {memory.salience.toFixed(2)}</span>
              </div>
              <p>{memory.content}</p>
              <button className="text-button" onClick={() => void openMemorySource(memory.source_path)} type="button">Open Markdown source</button>
            </details>
          ))}
        </aside>
      )}
      {contextInspection && (
        <p className="inline-status" role="status">
          {contextInspection.retrieval_mode} retrieval · {contextInspection.candidate_memories} candidate memories · {contextInspection.omitted_memories} omitted · {contextInspection.selected_memory_tokens} memory tokens · {contextInspection.estimated_input_tokens}/{contextInspection.input_token_limit} estimated input tokens · {contextInspection.reserved_output_tokens} reserved output tokens · {contextInspection.omitted_turns} history turns omitted
          {contextInspection.fallback_reason ? ` · ${contextInspection.fallback_reason}` : ""}
        </p>
      )}

      {resumeStatus && <p className="inline-status" role="status">{resumeStatus}</p>}
      {error && <p className="conversation-error" role="alert">{error}</p>}
      {latestAssistant && latestAssistant.status !== "complete" && (
        <button className="text-button retry-button" disabled={busy} onClick={retry} type="button">Retry response</button>
      )}
      <form className="composer" onSubmit={submit}>
        <textarea aria-label="Message" value={draft} onChange={(event) => setDraft(event.target.value)} placeholder="Write a message…" rows={3} />
        <div className="composer-footer">
          <span>{busy ? "Waiting for local model…" : "Saved locally after each turn"}</span>
          {busy && <button className="outline-button" onClick={cancel} type="button">Cancel</button>}
          <button className="primary-button" disabled={busy || !draft.trim() || loadedVaultRoot !== vaultRoot.trim()} type="submit">
            {busy ? "Thinking…" : "Send message"}
          </button>
        </div>
      </form>
    </section>
  );
}

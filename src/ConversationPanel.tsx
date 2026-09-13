import { FormEvent, useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { conversationClient, type CharacterDefinition, type ChatStreamEvent, type ContextInspection, type RequestSnapshot, type SessionSummary, type TranscriptDocument, type TranscriptTurn } from "./conversation";
import {
  localIdFromTitle,
  sceneClient,
  selectionFromTranscript,
  type LocalSummary,
  type SceneSettings,
  type SessionLocalSelection,
} from "./scene";
import type { RetrievedMemory } from "./conversation";
import { providerClient, type ProviderConfig } from "./providers";
import { initiativeClient, type InitiativeOutcome } from "./initiative";
import { memoryClient } from "./memory";
import { openPath } from "@tauri-apps/plugin-opener";
import {
  activeCharacterId,
  activeSessionId,
  activeSessionStorageKeys,
  rememberActiveSession,
  shellEvents,
} from "./activeSession";

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

function draftSession(sessionId: string): SessionSummary {
  return {
    session_id: sessionId,
    title: "",
    created_at: "",
    updated_at: "",
    turn_count: 0,
    preview: "New conversation",
    archived: false,
  };
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
  const [useHybridRetrieval, setUseHybridRetrieval] = useState(
    () => localStorage.getItem("tz-chatter.use-hybrid-retrieval") !== "false",
  );
  const [showContext, setShowContext] = useState(true);
  const [locals, setLocals] = useState<LocalSummary[]>([]);
  const [sceneSettings, setSceneSettings] = useState<SceneSettings>({
    schema_version: 1,
    default_local: null,
  });
  const [sessionLocal, setSessionLocal] = useState<SessionLocalSelection>({ kind: "default" });
  const [newLocalOpen, setNewLocalOpen] = useState(false);
  const [newLocalTitle, setNewLocalTitle] = useState("");
  const [newLocalBody, setNewLocalBody] = useState("");
  const transcriptEnd = useRef<HTMLDivElement | null>(null);
  const listFilter = useRef({ query: "", includeArchived: false });

  const publishSessions = useCallback(async (root: string, activeId: string) => {
    try {
      const { query, includeArchived } = listFilter.current;
      let listed = query.trim()
        ? await conversationClient.searchSessions(root, query)
        : await conversationClient.listSessions(root, includeArchived);
      if (!listed.some((session) => session.session_id === activeId)) {
        if (!query.trim()) {
          const known = includeArchived
            ? listed
            : await conversationClient.listSessions(root, true);
          const active = known.find((session) => session.session_id === activeId);
          listed = active ? [active, ...listed.filter((session) => session.session_id !== activeId)] : [draftSession(activeId), ...listed];
        }
      }
      window.dispatchEvent(new CustomEvent(shellEvents.sessionsUpdated, {
        detail: { sessions: listed, sessionId: activeId },
      }));
    } catch {
      window.dispatchEvent(new CustomEvent(shellEvents.sessionsUpdated, {
        detail: { sessions: [draftSession(activeId)], sessionId: activeId },
      }));
    }
  }, []);

  const loadSceneState = useCallback(async (root: string, transcript: TranscriptDocument | null) => {
    const [listed, settings] = await Promise.all([
      sceneClient.listLocals(root),
      sceneClient.loadSettings(root),
    ]);
    setLocals(listed);
    setSceneSettings(settings);
    setSessionLocal(selectionFromTranscript(transcript?.local));
    setNewLocalOpen(false);
  }, []);

  const applyResume = useCallback(async (
    root: string,
    result: { character: CharacterDefinition; transcript: TranscriptDocument | null },
    status: string,
  ) => {
    const nextSessionId = result.transcript?.session_id ?? crypto.randomUUID();
    setCharacter(result.character);
    setLoadedVaultRoot(root);
    setVaultRoot(root);
    setSessionId(nextSessionId);
    setTurns(result.transcript?.turns ?? []);
    setRetrievedMemories([]);
    setContextInspection(null);
    setLastRequest(null);
    rememberActiveSession(root, result.character.id, nextSessionId, result.character.name);
    setResumeStatus(status);
    try {
      await loadSceneState(root, result.transcript);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    }
    await publishSessions(root, nextSessionId);
  }, [loadSceneState, publishSessions]);

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
      await applyResume(
        trimmedRoot,
        result,
        result.transcript
          ? `Resumed ${result.transcript.turns.length} saved turn${result.transcript.turns.length === 1 ? "" : "s"}.`
          : `Loaded ${result.character.name}; a new conversation is ready.`,
      );
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
    } catch (requestError) {
      setResumeStatus(null);
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }, [applyResume]);

  const openSession = useCallback(async (nextSessionId: string) => {
    const root = (loadedVaultRoot ?? vaultRoot).trim();
    if (!root || busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await conversationClient.openSession(root, nextSessionId);
      await applyResume(
        root,
        result,
        `Opened ${result.transcript?.turns.length ?? 0} saved turn${(result.transcript?.turns.length ?? 0) === 1 ? "" : "s"}.`,
      );
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }, [applyResume, busy, loadedVaultRoot, vaultRoot]);

  const startSession = useCallback(async () => {
    const root = (loadedVaultRoot ?? vaultRoot).trim();
    if (!root || busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await conversationClient.startSession(root);
      await applyResume(root, result, `New conversation with ${result.character.name}.`);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }, [applyResume, busy, loadedVaultRoot, vaultRoot]);

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
    setUseHybridRetrieval(localStorage.getItem("tz-chatter.use-hybrid-retrieval") !== "false");
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => void loadSavedProvider(), 0);
    return () => window.clearTimeout(timer);
  }, [loadSavedProvider]);

  useEffect(() => {
    const onLoadVault = (event: Event) => {
      const nextRoot = (event as CustomEvent<string>).detail?.trim();
      if (nextRoot) void resumeVault(nextRoot);
    };
    const onOpenSession = (event: Event) => {
      const nextId = (event as CustomEvent<string>).detail;
      if (nextId) void openSession(nextId);
    };
    const onNewSession = () => {
      void startSession();
    };
    const onProviderChanged = () => {
      void loadSavedProvider();
    };
    const onSessionListFilter = (event: Event) => {
      const detail = (event as CustomEvent<{ query: string; includeArchived: boolean }>).detail;
      if (!detail) return;
      listFilter.current = {
        query: detail.query ?? "",
        includeArchived: Boolean(detail.includeArchived),
      };
      const root = (loadedVaultRoot ?? vaultRoot).trim();
      const activeId = activeSessionId() ?? sessionId;
      if (root && activeId) void publishSessions(root, activeId);
    };
    const onRenameSession = (event: Event) => {
      const detail = (event as CustomEvent<{ sessionId: string; title: string }>).detail;
      const root = (loadedVaultRoot ?? vaultRoot).trim();
      if (!detail?.sessionId || !root || busy) return;
      void conversationClient.renameSession(root, detail.sessionId, detail.title)
        .then(() => publishSessions(root, activeSessionId() ?? sessionId))
        .catch((requestError: unknown) => {
          setError(requestError instanceof Error ? requestError.message : String(requestError));
        });
    };
    const onArchiveSession = (event: Event) => {
      const detail = (event as CustomEvent<{ sessionId: string; archived: boolean }>).detail;
      const root = (loadedVaultRoot ?? vaultRoot).trim();
      if (!detail?.sessionId || !root || busy) return;
      void conversationClient.setSessionArchived(root, detail.sessionId, detail.archived)
        .then(() => publishSessions(root, activeSessionId() ?? sessionId))
        .catch((requestError: unknown) => {
          setError(requestError instanceof Error ? requestError.message : String(requestError));
        });
    };
    window.addEventListener(shellEvents.loadVault, onLoadVault);
    window.addEventListener(shellEvents.openSession, onOpenSession);
    window.addEventListener(shellEvents.newSession, onNewSession);
    window.addEventListener(shellEvents.providerChanged, onProviderChanged);
    window.addEventListener(shellEvents.sessionListFilter, onSessionListFilter);
    window.addEventListener(shellEvents.renameSession, onRenameSession);
    window.addEventListener(shellEvents.archiveSession, onArchiveSession);
    return () => {
      window.removeEventListener(shellEvents.loadVault, onLoadVault);
      window.removeEventListener(shellEvents.openSession, onOpenSession);
      window.removeEventListener(shellEvents.newSession, onNewSession);
      window.removeEventListener(shellEvents.providerChanged, onProviderChanged);
      window.removeEventListener(shellEvents.sessionListFilter, onSessionListFilter);
      window.removeEventListener(shellEvents.renameSession, onRenameSession);
      window.removeEventListener(shellEvents.archiveSession, onArchiveSession);
    };
  }, [busy, loadSavedProvider, loadedVaultRoot, openSession, publishSessions, resumeVault, sessionId, startSession, vaultRoot]);

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

  useEffect(() => {
    transcriptEnd.current?.scrollIntoView({ block: "end" });
  }, [turns]);

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
      await publishSessions(vaultRoot.trim(), sessionId);
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

  async function changeSessionLocal(value: string) {
    if (!vaultRoot.trim() || !sessionId) return;
    if (value === "__new") {
      setNewLocalOpen(true);
      return;
    }
    const selection: SessionLocalSelection =
      value === "default"
        ? { kind: "default" }
        : value === "none"
          ? { kind: "none" }
          : { kind: "local", id: value };
    setBusy(true);
    setError(null);
    try {
      const transcript = await conversationClient.setSessionLocal(vaultRoot.trim(), sessionId, selection);
      setSessionLocal(selectionFromTranscript(transcript.local));
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function createSessionLocal(event: FormEvent) {
    event.preventDefault();
    if (!vaultRoot.trim() || !sessionId || !newLocalTitle.trim()) return;
    const id = localIdFromTitle(newLocalTitle);
    if (!id) {
      setError("Local title must contain letters or numbers so it can become a file id.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await sceneClient.saveLocal(vaultRoot.trim(), {
        schema_version: 1,
        id,
        title: newLocalTitle.trim(),
        body: newLocalBody,
      });
      const transcript = await conversationClient.setSessionLocal(vaultRoot.trim(), sessionId, {
        kind: "local",
        id,
      });
      setNewLocalTitle("");
      setNewLocalBody("");
      setNewLocalOpen(false);
      await loadSceneState(vaultRoot.trim(), transcript);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  const latestAssistant = [...turns].reverse().find((turn) => turn.role === "assistant");
  const vaultReady = loadedVaultRoot === vaultRoot.trim() && Boolean(character.id);
  const sessionLocalValue =
    sessionLocal.kind === "default"
      ? "default"
      : sessionLocal.kind === "none"
        ? "none"
        : sessionLocal.id;
  const defaultLocalLabel = sceneSettings.default_local
    ? locals.find((entry) => entry.id === sceneSettings.default_local)?.title || sceneSettings.default_local
    : "none";

  return (
    <section className="conversation-panel" aria-label="Conversation">
      <header className="chat-header">
        <div>
          <h2>{vaultReady ? character.name : "Open a character to chat"}</h2>
          <p className="chat-header-meta">
            {vaultReady ? `${provider.chat_model} · ${turns.length} message${turns.length === 1 ? "" : "s"}` : "History appears here after you open a vault"}
          </p>
        </div>
        <div className="chat-header-actions">
          {vaultReady && (
            <label className="scene-picker">
              Scene
              <select
                aria-label="Session scene"
                disabled={busy}
                onChange={(event) => void changeSessionLocal(event.target.value)}
                value={sessionLocalValue}
              >
                <option value="default">Character default ({defaultLocalLabel})</option>
                <option value="none">None</option>
                {locals.map((entry) => (
                  <option key={entry.id} value={entry.id}>
                    {entry.title || entry.id}
                  </option>
                ))}
                <option value="__new">New local…</option>
              </select>
            </label>
          )}
          {retrievedMemories.length > 0 && (
            <button
              aria-pressed={showContext}
              className={showContext ? "outline-button active-toggle" : "outline-button"}
              onClick={() => setShowContext((current) => !current)}
              type="button"
            >
              Sources
            </button>
          )}
        </div>
      </header>

      <div className="chat-body">
        <div className="transcript" aria-live="polite">
          {!vaultReady ? (
            <VaultEmptyState
              busy={busy}
              onOpen={(root) => {
                setVaultRoot(root);
                void resumeVault(root);
              }}
              vaultRoot={vaultRoot}
            />
          ) : turns.length === 0 ? (
            <div className="empty-transcript">
              <span className="section-kicker">READY WHEN YOU ARE</span>
              <h2>Say hello to {character.name}.</h2>
              <p>Your first exchange will create a portable Markdown transcript in this conversation.</p>
            </div>
          ) : turns.map((turn) => (
            <article className={`message ${turn.role}`} key={turn.id}>
              <div className="message-meta">{roleLabel(turn, character.name)} · {turn.status}</div>
              <p>{turn.content || (turn.status === "failed" ? "The model did not return a response." : "Response interrupted.")}</p>
            </article>
          ))}
          <div ref={transcriptEnd} />
        </div>

        {showContext && retrievedMemories.length > 0 && (
          <aside className="context-inspector" aria-label="Retrieved memory context">
            <div className="context-inspector-header">
              <span className="section-kicker">CONTEXT USED</span>
              <span>{retrievedMemories.length} source{retrievedMemories.length === 1 ? "" : "s"} · ~{retrievedMemories.reduce((total, memory) => total + memory.estimated_tokens, 0)} tokens</span>
            </div>
            {retrievedMemories.map((memory) => (
              <details className="context-source" key={memory.memory_id}>
                <summary>{memory.memory_id} · {memory.source_path} · ~{memory.estimated_tokens} tokens</summary>
                <div className="context-source-meta">
                  <span>{memory.reasons.map((reason) => reason.split("_").join(" ")).join(" / ")}</span>
                  <span>Salience {memory.salience.toFixed(2)}</span>
                </div>
                <p>{memory.content}</p>
                <button className="text-button" onClick={() => void openMemorySource(memory.source_path)} type="button">Open Markdown source</button>
              </details>
            ))}
            {contextInspection && (
              <p className="inline-status" role="status">
                {contextInspection.retrieval_mode} retrieval · {contextInspection.candidate_memories} candidate memories · {contextInspection.omitted_memories} omitted · {contextInspection.selected_memory_tokens} memory tokens · {contextInspection.estimated_input_tokens}/{contextInspection.input_token_limit} estimated input tokens · {contextInspection.reserved_output_tokens} reserved output tokens · {contextInspection.omitted_turns} history turns omitted
                {contextInspection.fallback_reason ? ` · ${contextInspection.fallback_reason}` : ""}
              </p>
            )}
          </aside>
        )}
      </div>

      {newLocalOpen && vaultReady && (
        <form className="new-local-form" onSubmit={(event) => void createSessionLocal(event)}>
          <label>
            New local title
            <input
              onChange={(event) => setNewLocalTitle(event.target.value)}
              placeholder="Evening cafe"
              value={newLocalTitle}
            />
          </label>
          <label>
            Scene notes
            <textarea
              onChange={(event) => setNewLocalBody(event.target.value)}
              placeholder="Where you are tonight. Saved to locals/ before this session can use it."
              rows={3}
              value={newLocalBody}
            />
          </label>
          <div className="action-row">
            <button className="primary-button" disabled={busy || !newLocalTitle.trim()} type="submit">
              Create and use
            </button>
            <button className="text-button" onClick={() => setNewLocalOpen(false)} type="button">
              Cancel
            </button>
          </div>
        </form>
      )}
      {resumeStatus && <p className="inline-status chat-status" role="status">{resumeStatus}</p>}
      {error && <p className="conversation-error" role="alert">{error}</p>}
      {latestAssistant && latestAssistant.status !== "complete" && (
        <button className="text-button retry-button" disabled={busy} onClick={() => void retry()} type="button">Retry response</button>
      )}
      <form className="composer" onSubmit={submit}>
        <textarea
          aria-label="Message"
          disabled={!vaultReady}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              event.currentTarget.form?.requestSubmit();
            }
          }}
          placeholder={vaultReady ? `Message ${character.name}` : "Open a character vault to start chatting"}
          rows={3}
          value={draft}
        />
        <div className="composer-footer">
          <span>{busy ? "Waiting for local model…" : vaultReady ? "Enter to send · Shift+Enter for a new line" : "Saved locally after each turn"}</span>
          {busy && <button className="outline-button" onClick={() => void cancel()} type="button">Cancel</button>}
          <button className="primary-button" disabled={busy || !draft.trim() || !vaultReady} type="submit">
            {busy ? "Thinking…" : "Send"}
          </button>
        </div>
      </form>
    </section>
  );
}

function VaultEmptyState({
  busy,
  onOpen,
  vaultRoot,
}: {
  busy: boolean;
  onOpen: (root: string) => void;
  vaultRoot: string;
}) {
  const [draftRoot, setDraftRoot] = useState(vaultRoot);

  return (
    <div className="empty-transcript">
      <span className="section-kicker">START HERE</span>
      <h2>Open a character vault.</h2>
      <p>Pick the folder that contains character.md, or create a character from the Characters tab. Past chats from that vault will appear in the sidebar.</p>
      <form
        className="vault-open-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (!draftRoot.trim()) return;
          onOpen(draftRoot.trim());
        }}
      >
        <label>
          Vault folder
          <input
            onChange={(event) => setDraftRoot(event.target.value)}
            placeholder="C:\\Users\\you\\character-vault"
            value={draftRoot}
          />
        </label>
        <button className="primary-button" disabled={busy || !draftRoot.trim()} type="submit">
          Open vault
        </button>
      </form>
    </div>
  );
}

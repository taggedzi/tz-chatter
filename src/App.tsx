import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CharacterPanel } from "./CharacterPanel";
import { CharacterSwitcher } from "./CharacterSwitcher";
import { ConversationPanel } from "./ConversationPanel";
import { MemoryPanel } from "./MemoryPanel";
import { SettingsPanel, type SettingsSection } from "./SettingsPanel";
import {
  activeCharacterName,
  activeSessionChangedEvent,
  activeSessionId,
  activeSessionStorageKeys,
  requestArchiveSession,
  requestLoadVault,
  requestNewSession,
  requestOpenSession,
  requestRenameSession,
  requestSessionListFilter,
  shellEvents,
  type ConversationUiState,
} from "./activeSession";
import type { SessionSummary } from "./conversation";
import { characterClient } from "./characters";
import { CharacterPortraitMark } from "./CharacterPortrait";
import {
  fromKeyboardEvent,
  isMacPlatform,
  resolveChatShortcut,
  type ChatShortcutAction,
} from "./chatShortcuts";
import "./App.css";

function focusComposerSoon() {
  window.requestAnimationFrame(() => {
    window.dispatchEvent(new CustomEvent(shellEvents.focusComposer));
  });
}

type AppInfo = {
  name: string;
  version: string;
  stage: string;
};

type AppView = "chat" | "characters" | "memories";

function formatSessionTime(value: string) {
  if (!value) return "";
  const asNumber = Number(value);
  const date = Number.isFinite(asNumber) && asNumber > 1e12
    ? new Date(asNumber)
    : Number.isFinite(asNumber) && asNumber > 1e9
      ? new Date(asNumber)
      : new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const now = new Date();
  if (date.toDateString() === now.toDateString()) {
    return date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  }
  return date.toLocaleDateString([], { month: "short", day: "numeric" });
}

function sameVault(left: string, right: string) {
  return left.replace(/[\\/]+$/, "").toLowerCase() === right.replace(/[\\/]+$/, "").toLowerCase();
}

function sessionHeading(session: SessionSummary) {
  return session.title.trim() || session.preview.trim() || "New conversation";
}

function App() {
  const [activeView, setActiveView] = useState<AppView>("chat");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("provider");
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [shellError, setShellError] = useState<string | null>(null);
  const [activeCharacter, setActiveCharacter] = useState(() => activeCharacterName());
  const [vaultRoot, setVaultRoot] = useState(() => localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "");
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [sessionId, setSessionId] = useState(() => activeSessionId() ?? "");
  const [portraitSrc, setPortraitSrc] = useState<string | null>(null);
  const [sessionQuery, setSessionQuery] = useState("");
  const [showArchived, setShowArchived] = useState(false);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [switcherOpen, setSwitcherOpen] = useState(false);
  const skipRenameBlur = useRef(false);
  const conversationUi = useRef<ConversationUiState>({
    generating: false,
    sourcesOpen: false,
    sourcesAvailable: false,
    emojiPickerOpen: false,
    newLocalOpen: false,
  });
  const isMac = typeof navigator !== "undefined" && isMacPlatform(navigator.platform);
  const modifierShortcut = isMac ? "Meta" : "Control";

  const applyShortcut = useCallback((action: ChatShortcutAction) => {
    if (action === "close-switcher") {
      setSwitcherOpen(false);
      return;
    }
    if (action === "toggle-character-switcher") {
      setSwitcherOpen((open) => !open);
      return;
    }
    if (action === "close-settings") {
      setSettingsOpen(false);
      return;
    }
    if (action === "new-session") {
      setSwitcherOpen(false);
      setSettingsOpen(false);
      setActiveView("chat");
      requestNewSession();
      focusComposerSoon();
      return;
    }
    if (action === "focus-composer") {
      setSwitcherOpen(false);
      setSettingsOpen(false);
      setActiveView("chat");
      focusComposerSoon();
      return;
    }
    if (action === "stop-generation") {
      window.dispatchEvent(new CustomEvent(shellEvents.stopGeneration));
      return;
    }
    if (action === "toggle-sources") {
      setSwitcherOpen(false);
      setSettingsOpen(false);
      setActiveView("chat");
      window.dispatchEvent(new CustomEvent(shellEvents.toggleSources));
      return;
    }
    if (action === "close-sources") {
      window.dispatchEvent(new CustomEvent(shellEvents.closeSources));
      return;
    }
    if (action === "close-emoji-picker") {
      window.dispatchEvent(new CustomEvent(shellEvents.closeEmojiPicker));
      return;
    }
    if (action === "close-new-local") {
      window.dispatchEvent(new CustomEvent(shellEvents.closeNewLocal));
    }
  }, []);

  useEffect(() => {
    invoke<AppInfo>("app_info")
      .then(setAppInfo)
      .catch(() => setShellError("Rust core is unavailable"));
  }, []);

  useEffect(() => {
    const onUi = (event: Event) => {
      const detail = (event as CustomEvent<ConversationUiState>).detail;
      if (detail) conversationUi.current = detail;
    };
    const onKey = (event: KeyboardEvent) => {
      const action = resolveChatShortcut(fromKeyboardEvent(event), {
        switcherOpen,
        settingsOpen,
        renameOpen: renamingId !== null,
        ...conversationUi.current,
      }, { isMac: isMacPlatform(navigator.platform) });
      if (!action) return;
      event.preventDefault();
      applyShortcut(action);
    };
    window.addEventListener(shellEvents.conversationUi, onUi);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener(shellEvents.conversationUi, onUi);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [applyShortcut, renamingId, settingsOpen, switcherOpen]);

  useEffect(() => {
    const update = () => {
      setActiveCharacter(activeCharacterName());
      setVaultRoot(localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "");
      setSessionId(activeSessionId() ?? "");
    };
    const onSessions = (event: Event) => {
      const detail = (event as CustomEvent<{ sessions: SessionSummary[]; sessionId: string }>).detail;
      if (!detail) return;
      setSessions(detail.sessions);
      setSessionId(detail.sessionId);
    };
    window.addEventListener(activeSessionChangedEvent, update);
    window.addEventListener(shellEvents.sessionsUpdated, onSessions);
    return () => {
      window.removeEventListener(activeSessionChangedEvent, update);
      window.removeEventListener(shellEvents.sessionsUpdated, onSessions);
    };
  }, []);

  useEffect(() => {
    requestSessionListFilter(sessionQuery, showArchived);
  }, [sessionQuery, showArchived]);

  useEffect(() => {
    let cancelled = false;
    async function loadPortrait(root: string) {
      if (!root.trim()) {
        if (!cancelled) setPortraitSrc(null);
        return;
      }
      try {
        const src = await characterClient.loadPortrait(root);
        if (!cancelled) setPortraitSrc(src);
      } catch {
        if (!cancelled) setPortraitSrc(null);
      }
    }
    void loadPortrait(vaultRoot);
    const onFocus = () => {
      void loadPortrait(vaultRoot);
    };
    const onPortrait = (event: Event) => {
      const detail = (event as CustomEvent<{ vaultRoot: string }>).detail;
      if (detail && sameVault(detail.vaultRoot, vaultRoot)) {
        void loadPortrait(vaultRoot);
      }
    };
    window.addEventListener("focus", onFocus);
    window.addEventListener(shellEvents.portraitChanged, onPortrait);
    return () => {
      cancelled = true;
      window.removeEventListener("focus", onFocus);
      window.removeEventListener(shellEvents.portraitChanged, onPortrait);
    };
  }, [vaultRoot]);

  const characterLabel = activeCharacter ?? "No character loaded";

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="Workspace" aria-hidden={settingsOpen} inert={settingsOpen || undefined}>
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">tz</div>
          <div>
            <p className="eyebrow">LOCAL CHARACTER CHAT</p>
            <h1>tz-chatter</h1>
          </div>
        </div>

        <button
          className="character-card"
          onClick={() => { setSettingsOpen(false); setActiveView("characters"); }}
          type="button"
          aria-keyshortcuts={`${modifierShortcut}+K`}
          aria-label="Open character library"
        >
          {activeCharacter ? (
            <CharacterPortraitMark
              className="avatar"
              name={activeCharacter}
              src={portraitSrc}
            />
          ) : (
            <div aria-hidden="true" className="avatar">—</div>
          )}
          <div>
            <p className="eyebrow">ACTIVE CHARACTER</p>
            <h2>{characterLabel}</h2>
            <p className="muted">{activeCharacter ? "History stays in this vault" : "Create or open a character"}</p>
          </div>
        </button>

        <nav className="nav-list" aria-label="Primary">
          <button
            aria-current={activeView === "chat" && !settingsOpen ? "page" : undefined}
            className={activeView === "chat" && !settingsOpen ? "nav-item active" : "nav-item"}
            onClick={() => { setSettingsOpen(false); setActiveView("chat"); }}
            type="button"
          >
            <span className="nav-icon" aria-hidden="true">#</span>
            Chat
          </button>
          <button
            aria-current={activeView === "characters" && !settingsOpen ? "page" : undefined}
            className={activeView === "characters" && !settingsOpen ? "nav-item active" : "nav-item"}
            onClick={() => { setSettingsOpen(false); setActiveView("characters"); }}
            type="button"
          >
            <span className="nav-icon" aria-hidden="true">C</span>
            Characters
          </button>
          <button
            aria-current={activeView === "memories" && !settingsOpen ? "page" : undefined}
            className={activeView === "memories" && !settingsOpen ? "nav-item active" : "nav-item"}
            onClick={() => { setSettingsOpen(false); setActiveView("memories"); }}
            type="button"
          >
            <span className="nav-icon" aria-hidden="true">M</span>
            Memories
          </button>
          <button
            aria-current={settingsOpen ? "page" : undefined}
            className={settingsOpen ? "nav-item active" : "nav-item"}
            onClick={() => setSettingsOpen(true)}
            type="button"
          >
            <span className="nav-icon" aria-hidden="true">S</span>
            Settings
          </button>
        </nav>

        <section className="session-list" aria-label="Conversation history">
          <div className="session-list-header">
            <span className="eyebrow">Conversations</span>
            <button
              aria-keyshortcuts={`${modifierShortcut}+N`}
              className="text-button"
              disabled={!vaultRoot.trim() || !activeCharacter}
              onClick={() => {
                setSettingsOpen(false);
                setActiveView("chat");
                requestNewSession();
                focusComposerSoon();
              }}
              type="button"
            >
              New
            </button>
          </div>
          <label className="session-search">
            <span className="visually-hidden">Search conversations</span>
            <input
              disabled={!vaultRoot.trim() || !activeCharacter}
              onChange={(event) => setSessionQuery(event.target.value)}
              placeholder="Search chats"
              type="search"
              value={sessionQuery}
            />
          </label>
          {sessions.length === 0 && (
            <p className="muted session-empty">
              {activeCharacter
                ? sessionQuery.trim()
                  ? "No chats match that search."
                  : "No saved chats yet. Send a message to start one."
                : "Open a vault to see past chats."}
            </p>
          )}
          {sessions.map((session) => (
            <div
              className={[
                "session-row",
                session.session_id === sessionId ? "selected" : "",
                session.archived ? "archived" : "",
              ].filter(Boolean).join(" ")}
              key={session.session_id}
            >
              {renamingId === session.session_id ? (
                <form
                  className="session-rename"
                  onSubmit={(event) => {
                    event.preventDefault();
                    requestRenameSession(session.session_id, renameDraft);
                    setRenamingId(null);
                  }}
                >
                  <label>
                    <span className="visually-hidden">Session name</span>
                    <input
                      autoFocus
                      onBlur={() => {
                        if (skipRenameBlur.current) {
                          skipRenameBlur.current = false;
                          return;
                        }
                        requestRenameSession(session.session_id, renameDraft);
                        setRenamingId(null);
                      }}
                      onChange={(event) => setRenameDraft(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Escape") {
                          event.preventDefault();
                          skipRenameBlur.current = true;
                          setRenamingId(null);
                        }
                      }}
                      type="text"
                      value={renameDraft}
                    />
                  </label>
                </form>
              ) : (
                <button
                  className="session-row-main"
                  onClick={() => {
                    setSettingsOpen(false);
                    setActiveView("chat");
                    if (session.session_id !== sessionId) requestOpenSession(session.session_id);
                  }}
                  onDoubleClick={(event) => {
                    event.preventDefault();
                    setRenamingId(session.session_id);
                    setRenameDraft(sessionHeading(session));
                  }}
                  type="button"
                >
                  <span className="session-row-top">
                    <strong>{sessionHeading(session)}</strong>
                    <time>{formatSessionTime(session.updated_at)}</time>
                  </span>
                  <span className="session-row-meta">
                    {session.archived ? "Archived · " : ""}
                    {session.turn_count} message{session.turn_count === 1 ? "" : "s"}
                  </span>
                  {session.snippet ? (
                    <span className="session-row-snippet">{session.snippet}</span>
                  ) : null}
                </button>
              )}
              <div className="session-row-actions">
                <button
                  className="text-button"
                  disabled={!vaultRoot.trim()}
                  onClick={() => {
                    setRenamingId(session.session_id);
                    setRenameDraft(sessionHeading(session));
                  }}
                  type="button"
                >
                  Rename
                </button>
                <button
                  className="text-button"
                  disabled={!vaultRoot.trim()}
                  onClick={() => requestArchiveSession(session.session_id, !session.archived)}
                  type="button"
                >
                  {session.archived ? "Unarchive" : "Archive"}
                </button>
              </div>
            </div>
          ))}
          <label className="session-archive-toggle">
            <input
              checked={showArchived}
              disabled={!vaultRoot.trim() || !activeCharacter}
              onChange={(event) => setShowArchived(event.target.checked)}
              type="checkbox"
            />
            Show archived
          </label>
        </section>

        <div className="sidebar-footer">
          <span className={shellError ? "status-dot warning" : "status-dot"} />
          <span>{shellError ?? (appInfo ? "Desktop core connected" : "Connecting to desktop core…")}</span>
        </div>
      </aside>

      <main className="main-content" aria-hidden={settingsOpen} inert={settingsOpen || undefined}>
        <div className={activeView === "chat" ? "main-view" : "main-view hidden"} hidden={activeView !== "chat"}>
          <ConversationPanel />
        </div>
        <div className={activeView === "characters" ? "main-view" : "main-view hidden"} hidden={activeView !== "characters"}>
          <CharacterPanel active={!settingsOpen && activeView === "characters"} />
        </div>
        <div className={activeView === "memories" ? "main-view" : "main-view hidden"} hidden={activeView !== "memories"}>
          <MemoryPanel active={!settingsOpen && activeView === "memories"} />
        </div>
      </main>

      {settingsOpen && (
        <SettingsPanel
          onClose={() => setSettingsOpen(false)}
          onSection={setSettingsSection}
          section={settingsSection}
        />
      )}

      {switcherOpen && (
        <CharacterSwitcher
          activeVaultRoot={vaultRoot}
          onClose={() => setSwitcherOpen(false)}
          onSelect={(root) => {
            setSwitcherOpen(false);
            setSettingsOpen(false);
            setActiveView("chat");
            if (!sameVault(root, vaultRoot)) requestLoadVault(root);
            focusComposerSoon();
          }}
        />
      )}
    </div>
  );
}

export default App;

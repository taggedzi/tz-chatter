import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CharacterPanel } from "./CharacterPanel";
import { ConversationPanel } from "./ConversationPanel";
import { MemoryPanel } from "./MemoryPanel";
import { SettingsPanel, type SettingsSection } from "./SettingsPanel";
import {
  activeCharacterName,
  activeSessionChangedEvent,
  activeSessionId,
  activeSessionStorageKeys,
  requestNewSession,
  requestOpenSession,
  shellEvents,
} from "./activeSession";
import type { SessionSummary } from "./conversation";
import { characterClient } from "./characters";
import { CharacterPortraitMark } from "./CharacterPortrait";
import "./App.css";

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

  useEffect(() => {
    invoke<AppInfo>("app_info")
      .then(setAppInfo)
      .catch(() => setShellError("Rust core is unavailable"));
  }, []);

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
      <aside className="sidebar" aria-label="Workspace">
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
              className="text-button"
              disabled={!vaultRoot.trim() || !activeCharacter}
              onClick={() => {
                setSettingsOpen(false);
                setActiveView("chat");
                requestNewSession();
              }}
              type="button"
            >
              New
            </button>
          </div>
          {sessions.length === 0 && (
            <p className="muted session-empty">
              {activeCharacter ? "No saved chats yet. Send a message to start one." : "Open a vault to see past chats."}
            </p>
          )}
          {sessions.map((session) => (
            <button
              className={session.session_id === sessionId ? "session-row selected" : "session-row"}
              key={session.session_id}
              onClick={() => {
                setSettingsOpen(false);
                setActiveView("chat");
                if (session.session_id !== sessionId) requestOpenSession(session.session_id);
              }}
              type="button"
            >
              <span className="session-row-top">
                <strong>{session.preview || "New conversation"}</strong>
                <time>{formatSessionTime(session.updated_at)}</time>
              </span>
              <span className="session-row-meta">
                {session.turn_count} message{session.turn_count === 1 ? "" : "s"}
              </span>
            </button>
          ))}
        </section>

        <div className="sidebar-footer">
          <span className={shellError ? "status-dot warning" : "status-dot"} />
          <span>{shellError ?? (appInfo ? "Desktop core connected" : "Connecting to desktop core…")}</span>
        </div>
      </aside>

      <main className="main-content">
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
    </div>
  );
}

export default App;

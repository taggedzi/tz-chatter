import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
import "./App.css";

type AppInfo = {
  name: string;
  version: string;
  stage: string;
};

type AppView = "chat" | "memories";

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

function initials(name: string) {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "tz";
  return parts.slice(0, 2).map((part) => part[0]?.toUpperCase() ?? "").join("");
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

        <section className="character-card" aria-label="Active character">
          <div className="avatar">{activeCharacter ? initials(activeCharacter) : "—"}</div>
          <div>
            <p className="eyebrow">ACTIVE CHARACTER</p>
            <h2>{characterLabel}</h2>
            <p className="muted">{activeCharacter ? "History stays in this vault" : "Open a vault to begin"}</p>
          </div>
        </section>

        <nav className="nav-list" aria-label="Primary">
          <button
            className={activeView === "chat" && !settingsOpen ? "nav-item active" : "nav-item"}
            onClick={() => { setSettingsOpen(false); setActiveView("chat"); }}
            type="button"
          >
            <span className="nav-icon" aria-hidden="true">#</span>
            Chat
          </button>
          <button
            className={activeView === "memories" && !settingsOpen ? "nav-item active" : "nav-item"}
            onClick={() => { setSettingsOpen(false); setActiveView("memories"); }}
            type="button"
          >
            <span className="nav-icon" aria-hidden="true">M</span>
            Memories
          </button>
          <button
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

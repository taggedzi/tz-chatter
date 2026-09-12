import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ConversationPanel } from "./ConversationPanel";
import { InitiativePanel } from "./InitiativePanel";
import { PortabilityPanel } from "./PortabilityPanel";
import { MemoryPanel } from "./MemoryPanel";
import { activeCharacterName, activeSessionChangedEvent } from "./activeSession";
import "./App.css";

type AppInfo = {
  name: string;
  version: string;
  stage: string;
};

const navItems = ["Conversation", "Memories", "Settings"];

function App() {
  const [activeView, setActiveView] = useState("Conversation");
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [shellError, setShellError] = useState<string | null>(null);
  const [activeCharacter, setActiveCharacter] = useState(() => activeCharacterName());

  useEffect(() => {
    invoke<AppInfo>("app_info")
      .then(setAppInfo)
      .catch(() => setShellError("Rust core is unavailable"));
  }, []);

  useEffect(() => {
    const update = () => setActiveCharacter(activeCharacterName());
    window.addEventListener(activeSessionChangedEvent, update);
    return () => window.removeEventListener(activeSessionChangedEvent, update);
  }, []);

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="Primary navigation">
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">tz</div>
          <div>
            <p className="eyebrow">LOCAL CHARACTER CHAT</p>
            <h1>tz-chatter</h1>
          </div>
        </div>

        <section className="character-card" aria-label="Active character">
          <div className="avatar">—</div>
          <div>
            <p className="eyebrow">ACTIVE CHARACTER</p>
            <h2>{activeCharacter ?? "No character loaded"}</h2>
            <p className="muted">{activeCharacter ? "Active local vault" : "Choose a vault to begin"}</p>
          </div>
        </section>

        <nav className="nav-list">
          {navItems.map((item) => (
            <button
              className={activeView === item ? "nav-item active" : "nav-item"}
              key={item}
              onClick={() => setActiveView(item)}
              aria-current={activeView === item ? "page" : undefined}
              type="button"
            >
              <span className="nav-icon" aria-hidden="true">{item.slice(0, 1)}</span>
              {item}
            </button>
          ))}
        </nav>

        <div className="sidebar-footer">
          <span className={shellError ? "status-dot warning" : "status-dot"} />
          <span>{shellError ?? (appInfo ? "Desktop core connected" : "Connecting to desktop core…")}</span>
        </div>
      </aside>

      <main className="main-content">
        <header className="topbar">
          <div>
            <p className="eyebrow">{activeView.toUpperCase()}</p>
            <p className="topbar-title">Your private space for persistent conversations.</p>
          </div>
          <button className="outline-button" onClick={() => setActiveView("Conversation")} type="button">Open conversation</button>
          </header>
        {activeView === "Conversation" && <ConversationPanel />}
        {activeView === "Memories" && <MemoryPanel />}
        {activeView === "Settings" && (
          <>
            <InitiativePanel />
            <PortabilityPanel />
          </>
        )}

        <section className="welcome-panel">
          <div className="welcome-copy">
            <span className="section-kicker">{appInfo?.stage ?? "INITIAL SETUP"}</span>
            <h2>Bring a character to life.</h2>
            <p>
              Connect a local model, load a portable character vault, and build a conversation
              that remembers what matters.
            </p>
            <div className="action-row">
              <button className="primary-button" onClick={() => setActiveView("Conversation")} type="button">Open a character vault</button>
              <button className="text-button" onClick={() => setActiveView("Settings")} type="button">Learn about vaults <span aria-hidden="true">→</span></button>
            </div>
          </div>
          <div className="welcome-orbit" aria-hidden="true">
            <div className="orbit orbit-one" />
            <div className="orbit orbit-two" />
            <div className="orbit-core">✦</div>
          </div>
        </section>

        <section className="workspace-grid" aria-label="Getting started">
          <article className="info-card">
            <div className="card-number">01</div>
            <h3>Connect locally</h3>
            <p>Use Ollama, llama.cpp, or LM Studio without sending your character data to the cloud.</p>
          </article>
          <article className="info-card highlighted">
            <div className="card-number">02</div>
            <h3>Keep memories portable</h3>
            <p>Identity, memories, and transcripts stay readable in Markdown inside your vault.</p>
          </article>
          <article className="info-card">
            <div className="card-number">03</div>
            <h3>Stay in control</h3>
            <p>Inspect context, correct memories, and decide when optional initiative is enabled.</p>
          </article>
        </section>

        <footer className="app-footer">
          <span>Local-first by design</span>
          <span>{appInfo ? `v${appInfo.version}` : "Preparing shell"}</span>
        </footer>
      </main>
    </div>
  );
}

export default App;

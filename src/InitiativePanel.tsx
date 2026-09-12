import { useEffect, useState } from "react";
import { initiativeClient, type InitiativeSettings, type InitiativeSnapshot } from "./initiative";
import { activeCharacterId, activeSessionChangedEvent, activeSessionId, activeSessionStorageKeys } from "./activeSession";

const defaultSettings: InitiativeSettings = {
  schema_version: 1,
  enabled: false,
  notifications_enabled: false,
  min_inactive_seconds: 900,
  cooldown_seconds: 1800,
  max_per_day: 3,
  max_ignored: 3,
  quiet_start_minute: 1320,
  quiet_end_minute: 420,
};

function formatMinute(minute: number) {
  return `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
}

function parseMinute(value: string) {
  const [hours, minutes] = value.split(":").map(Number);
  return hours * 60 + minutes;
}

export function InitiativePanel() {
  const [vaultRoot, setVaultRoot] = useState(() => localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "");
  const [characterId, setCharacterId] = useState(() => activeCharacterId() ?? "");
  const [sessionId, setSessionId] = useState(() => activeSessionId() ?? "");
  const [settings, setSettings] = useState(defaultSettings);
  const [snapshot, setSnapshot] = useState<InitiativeSnapshot | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    const sync = () => {
      setVaultRoot(localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "");
      setCharacterId(activeCharacterId() ?? "");
      setSessionId(activeSessionId() ?? "");
    };
    window.addEventListener(activeSessionChangedEvent, sync);
    return () => window.removeEventListener(activeSessionChangedEvent, sync);
  }, []);

  useEffect(() => {
    if (!vaultRoot.trim() || !characterId.trim()) return undefined;
    let disposed = false;
    void initiativeClient
      .snapshot(vaultRoot, characterId, Math.floor(Date.now() / 1000))
      .then((result) => {
        if (disposed) return;
        setSnapshot(result);
        setSettings(result.settings);
      })
      .catch(() => {
        // The explicit Check eligibility action remains available when the vault is unavailable.
      });
    return () => {
      disposed = true;
    };
  }, [vaultRoot, characterId]);

  async function save() {
    if (!vaultRoot.trim() || !characterId.trim()) {
      setStatus("Choose a vault folder and character before saving.");
      return;
    }
    if (settings.enabled && !sessionId) {
      setStatus("Load a conversation before enabling initiative.");
      return;
    }
    try {
      await initiativeClient.saveSettings(vaultRoot.trim(), characterId.trim(), settings);
      if (settings.enabled) {
        await initiativeClient.startScheduler(vaultRoot.trim(), characterId.trim(), sessionId);
        setStatus("Initiative settings saved; scheduler started.");
      } else {
        await initiativeClient.stopScheduler(characterId.trim(), sessionId);
        setStatus("Initiative settings saved; scheduler stopped.");
      }
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function checkEligibility() {
    if (!vaultRoot.trim() || !characterId.trim()) {
      setStatus("Choose a vault folder and character before checking.");
      return;
    }
    try {
      const result = await initiativeClient.snapshot(
        vaultRoot.trim(),
        characterId.trim(),
        Math.floor(Date.now() / 1000),
      );
      setSnapshot(result);
      setSettings(result.settings);
      setStatus("Eligibility evaluated by the desktop core.");
    } catch (error) {
      setStatus(String(error));
    }
  }

  return (
    <section className="settings-section">
      <div className="section-heading">
        <div>
          <span className="section-kicker">OPTIONAL INITIATIVE</span>
          <h2>Let the character reach out thoughtfully.</h2>
        </div>
        <span className="pill">Off by default</span>
      </div>
      <p className="panel-description">
        Applies to the loaded character. Eligibility is checked before inference. Quiet hours, inactivity,
        cooldowns, caps, unanswered messages, and ignored-message backoff remain enforced after restart.
      </p>
      {!characterId.trim() && <p className="inline-status">Open a vault from the sidebar before enabling initiative.</p>}
      <div className="settings-grid">
        <label className="checkbox-row">
          <input
            checked={settings.enabled}
            onChange={(event) => setSettings({ ...settings, enabled: event.target.checked })}
            type="checkbox"
          />
          Enable spontaneous messages
        </label>
        <label className="checkbox-row">
          <input
            checked={settings.notifications_enabled}
            onChange={(event) => setSettings({ ...settings, notifications_enabled: event.target.checked })}
            type="checkbox"
          />
          Show desktop notifications for delivered messages
        </label>
        <label>
          Inactive for (minutes)
          <input
            min={0}
            type="number"
            value={Math.round(settings.min_inactive_seconds / 60)}
            onChange={(event) => setSettings({ ...settings, min_inactive_seconds: Number(event.target.value) * 60 })}
          />
        </label>
        <label>
          Cooldown (minutes)
          <input
            min={0}
            type="number"
            value={Math.round(settings.cooldown_seconds / 60)}
            onChange={(event) => setSettings({ ...settings, cooldown_seconds: Number(event.target.value) * 60 })}
          />
        </label>
        <label>
          Maximum per day
          <input
            min={1}
            type="number"
            value={settings.max_per_day}
            onChange={(event) => setSettings({ ...settings, max_per_day: Number(event.target.value) })}
          />
        </label>
        <label>
          Stop after ignored
          <input
            min={1}
            type="number"
            value={settings.max_ignored}
            onChange={(event) => setSettings({ ...settings, max_ignored: Number(event.target.value) })}
          />
        </label>
        <label>
          Quiet hours start
          <input
            type="time"
            value={formatMinute(settings.quiet_start_minute)}
            onChange={(event) => setSettings({ ...settings, quiet_start_minute: parseMinute(event.target.value) })}
          />
        </label>
        <label>
          Quiet hours end
          <input
            type="time"
            value={formatMinute(settings.quiet_end_minute)}
            onChange={(event) => setSettings({ ...settings, quiet_end_minute: parseMinute(event.target.value) })}
          />
        </label>
      </div>
      <div className="action-row">
        <button className="primary-button" onClick={save} type="button">Save settings</button>
        <button className="outline-button" onClick={checkEligibility} type="button">Check eligibility</button>
      </div>
      <p className="field-help">Saving with initiative enabled starts the local 30-second scheduler for the active conversation. It resumes after restart; disable it and save to stop this session.</p>
      {status && <p className="inline-status">{status}</p>}
      {snapshot && (
        <div className="initiative-result">
          <strong>{snapshot.decision.eligible ? "Eligible when the scheduler runs." : "Not eligible right now."}</strong>
          <span>
            {snapshot.decision.reasons.length > 0
              ? snapshot.decision.reasons.map((reason) => reason.split("_").join(" ")).join(" / ")
              : "All gates passed"}
          </span>
          <span>Sent today: {snapshot.state.sent_today} · Ignored streak: {snapshot.state.ignored_streak}</span>
        </div>
      )}
    </section>
  );
}

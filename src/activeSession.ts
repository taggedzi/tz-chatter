export const activeSessionStorageKeys = {
  vaultRoot: "tz-chatter.vault-root",
  characterId: "tz-chatter.active-character-id",
  characterName: "tz-chatter.active-character-name",
  sessionId: "tz-chatter.active-session-id",
} as const;

export const activeSessionChangedEvent = "tz-chatter-active-session-changed";

export const shellEvents = {
  loadVault: "tz-chatter-load-vault",
  openSession: "tz-chatter-open-session",
  newSession: "tz-chatter-new-session",
  sessionsUpdated: "tz-chatter-sessions-updated",
  providerChanged: "tz-chatter-provider-changed",
  portraitChanged: "tz-chatter-portrait-changed",
} as const;

export function requestLoadVault(vaultRoot: string) {
  window.dispatchEvent(new CustomEvent(shellEvents.loadVault, { detail: vaultRoot }));
}

export function requestOpenSession(sessionId: string) {
  window.dispatchEvent(new CustomEvent(shellEvents.openSession, { detail: sessionId }));
}

export function requestNewSession() {
  window.dispatchEvent(new CustomEvent(shellEvents.newSession));
}

export function notifyProviderChanged() {
  window.dispatchEvent(new CustomEvent(shellEvents.providerChanged));
}

export function notifyPortraitChanged(vaultRoot: string) {
  window.dispatchEvent(new CustomEvent(shellEvents.portraitChanged, { detail: { vaultRoot } }));
}

export function rememberActiveSession(vaultRoot: string, characterId: string, sessionId: string, characterName = characterId) {
  localStorage.setItem(activeSessionStorageKeys.vaultRoot, vaultRoot);
  localStorage.setItem(activeSessionStorageKeys.characterId, characterId);
  localStorage.setItem(activeSessionStorageKeys.characterName, characterName);
  localStorage.setItem(activeSessionStorageKeys.sessionId, sessionId);
  window.dispatchEvent(new CustomEvent(activeSessionChangedEvent, {
    detail: { vaultRoot, characterId, characterName, sessionId },
  }));
}

export function activeSessionId() {
  return localStorage.getItem(activeSessionStorageKeys.sessionId);
}

export function activeCharacterId() {
  return localStorage.getItem(activeSessionStorageKeys.characterId);
}

export function activeCharacterName() {
  return localStorage.getItem(activeSessionStorageKeys.characterName);
}

export const activeSessionStorageKeys = {
  vaultRoot: "tz-chatter.vault-root",
  characterId: "tz-chatter.active-character-id",
  characterName: "tz-chatter.active-character-name",
  sessionId: "tz-chatter.active-session-id",
} as const;

export const activeSessionChangedEvent = "tz-chatter-active-session-changed";

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

export function initiativeAvailability(
  vaultRoot: string,
  characterId: string,
  sessionId: string,
  enabled: boolean,
) {
  const characterUnavailable = !vaultRoot.trim() || !characterId.trim();
  const enableUnavailable = characterUnavailable || !sessionId;
  return {
    characterUnavailable,
    enableUnavailable,
    saveUnavailable: characterUnavailable || (enabled && !sessionId),
  };
}

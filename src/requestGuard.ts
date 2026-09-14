export type ChatRequestIdentity = {
  vaultRoot: string;
  characterId: string;
  sessionId: string;
  requestId: string;
};

export function identitiesMatch(
  current: ChatRequestIdentity | null | undefined,
  incoming: ChatRequestIdentity | null | undefined,
) {
  if (!current || !incoming) return false;
  return current.vaultRoot === incoming.vaultRoot
    && current.characterId === incoming.characterId
    && current.sessionId === incoming.sessionId
    && current.requestId === incoming.requestId;
}

export function shouldApplyChatUpdate(
  current: ChatRequestIdentity | null | undefined,
  incoming: ChatRequestIdentity | null | undefined,
) {
  return identitiesMatch(current, incoming);
}

export function shouldClearDraft(currentDraft: string, submitted: string) {
  return currentDraft.trim() === submitted.trim();
}

export function composerShouldSubmit(event: {
  key: string;
  shiftKey: boolean;
  isComposing?: boolean;
  nativeEvent?: { isComposing?: boolean };
}) {
  if (event.key !== "Enter" || event.shiftKey) return false;
  if (event.isComposing || event.nativeEvent?.isComposing) return false;
  return true;
}

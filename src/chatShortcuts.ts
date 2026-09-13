export type ChatShortcutAction =
  | "new-session"
  | "toggle-character-switcher"
  | "focus-composer"
  | "stop-generation"
  | "toggle-sources"
  | "close-switcher"
  | "close-settings"
  | "close-sources"
  | "close-new-local";

export type ChatShortcutUi = {
  switcherOpen: boolean;
  settingsOpen: boolean;
  renameOpen: boolean;
  newLocalOpen: boolean;
  generating: boolean;
  sourcesOpen: boolean;
  sourcesAvailable: boolean;
};

export type ChatShortcutEvent = {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  repeat: boolean;
  isComposing: boolean;
};

export function isMacPlatform(platform: string) {
  return /Mac|iPhone|iPad|iPod/i.test(platform);
}

export function shortcutModifierLabel(isMac: boolean) {
  return isMac ? "⌘" : "Ctrl";
}

export function fromKeyboardEvent(event: KeyboardEvent): ChatShortcutEvent {
  return {
    key: event.key,
    code: event.code,
    ctrlKey: event.ctrlKey,
    metaKey: event.metaKey,
    altKey: event.altKey,
    shiftKey: event.shiftKey,
    repeat: event.repeat,
    isComposing: event.isComposing,
  };
}

function hasPrimaryModifier(event: ChatShortcutEvent, isMac: boolean) {
  return isMac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}

function matchesCode(event: ChatShortcutEvent, code: string, letters: string[]) {
  if (event.code === code) return true;
  return letters.includes(event.key);
}

export function resolveChatShortcut(
  event: ChatShortcutEvent,
  ui: ChatShortcutUi,
  options: { isMac?: boolean } = {},
): ChatShortcutAction | null {
  if (event.isComposing || event.altKey) return null;

  if (event.key === "Escape" || event.code === "Escape") {
    if (ui.switcherOpen) return "close-switcher";
    if (ui.settingsOpen) return "close-settings";
    if (ui.renameOpen) return null;
    if (ui.newLocalOpen) return "close-new-local";
    if (ui.generating) return "stop-generation";
    if (ui.sourcesOpen) return "close-sources";
    return "focus-composer";
  }

  const isMac = options.isMac === true;
  if (!hasPrimaryModifier(event, isMac) || event.shiftKey) return null;

  if (matchesCode(event, "KeyN", ["n", "N"])) {
    return event.repeat ? null : "new-session";
  }
  if (matchesCode(event, "KeyK", ["k", "K"])) return "toggle-character-switcher";
  if (matchesCode(event, "KeyL", ["l", "L"])) return "focus-composer";
  if (matchesCode(event, "KeyI", ["i", "I"])) {
    return ui.sourcesAvailable ? "toggle-sources" : null;
  }
  if (matchesCode(event, "Period", ["."])) {
    return ui.generating ? "stop-generation" : null;
  }
  return null;
}

export function filterCharacterChoices<T extends { name: string; character_id: string; error: string | null }>(
  entries: T[],
  query: string,
) {
  const needle = query.trim().toLowerCase();
  const usable = entries.filter((entry) => !entry.error);
  if (!needle) return usable;
  return usable.filter((entry) => (
    entry.name.toLowerCase().includes(needle)
    || entry.character_id.toLowerCase().includes(needle)
  ));
}

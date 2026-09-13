import assert from "node:assert/strict";
import { test } from "node:test";
import {
  filterCharacterChoices,
  isMacPlatform,
  resolveChatShortcut,
} from "../src/chatShortcuts.ts";

function key(partial) {
  return {
    key: "",
    code: "",
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    repeat: false,
    isComposing: false,
    ...partial,
  };
}

function ui(partial = {}) {
  return {
    switcherOpen: false,
    settingsOpen: false,
    renameOpen: false,
    emojiPickerOpen: false,
    newLocalOpen: false,
    generating: false,
    sourcesOpen: false,
    sourcesAvailable: false,
    ...partial,
  };
}

const ctrlN = key({ code: "KeyN", key: "n", ctrlKey: true });
const cmdN = key({ code: "KeyN", key: "n", metaKey: true });

test("Ctrl+N starts a new session on Windows", () => {
  assert.equal(resolveChatShortcut(ctrlN, ui()), "new-session");
});

test("ignores key-repeat on new session", () => {
  assert.equal(resolveChatShortcut({ ...ctrlN, repeat: true }, ui()), null);
});

test("ignores Shift or Alt chords", () => {
  assert.equal(resolveChatShortcut({ ...ctrlN, shiftKey: true }, ui()), null);
  assert.equal(resolveChatShortcut({ ...ctrlN, altKey: true }, ui()), null);
});

test("ignores IME composition", () => {
  assert.equal(resolveChatShortcut({ ...ctrlN, isComposing: true }, ui()), null);
});

test("Cmd+N starts a new session on Mac, Ctrl+N does not", () => {
  assert.equal(resolveChatShortcut(cmdN, ui(), { isMac: true }), "new-session");
  assert.equal(resolveChatShortcut(ctrlN, ui(), { isMac: true }), null);
  assert.equal(resolveChatShortcut(cmdN, ui(), { isMac: false }), null);
});

test("Ctrl+K toggles the character switcher", () => {
  assert.equal(
    resolveChatShortcut(key({ code: "KeyK", key: "k", ctrlKey: true }), ui()),
    "toggle-character-switcher",
  );
  assert.equal(
    resolveChatShortcut(key({ code: "KeyK", key: "k", ctrlKey: true }), ui({ switcherOpen: true })),
    "toggle-character-switcher",
  );
});

test("Ctrl+L focuses the composer", () => {
  assert.equal(
    resolveChatShortcut(key({ code: "KeyL", key: "l", ctrlKey: true }), ui()),
    "focus-composer",
  );
});

test("Ctrl+. stops generation only while a reply is in flight", () => {
  const period = key({ code: "Period", key: ".", ctrlKey: true });
  assert.equal(resolveChatShortcut(period, ui({ generating: true })), "stop-generation");
  assert.equal(resolveChatShortcut(period, ui()), null);
});

test("Ctrl+I toggles sources only when memories were retrieved", () => {
  const chord = key({ code: "KeyI", key: "i", ctrlKey: true });
  assert.equal(resolveChatShortcut(chord, ui({ sourcesAvailable: true })), "toggle-sources");
  assert.equal(resolveChatShortcut(chord, ui()), null);
});

test("Escape closes the switcher before Settings or stop", () => {
  assert.equal(
    resolveChatShortcut(key({ key: "Escape", code: "Escape" }), ui({
      switcherOpen: true,
      settingsOpen: true,
      generating: true,
    })),
    "close-switcher",
  );
});

test("Escape closes Settings before stopping generation", () => {
  assert.equal(
    resolveChatShortcut(key({ key: "Escape", code: "Escape" }), ui({
      settingsOpen: true,
      generating: true,
    })),
    "close-settings",
  );
});

test("Escape leaves session rename to the rename field", () => {
  assert.equal(
    resolveChatShortcut(key({ key: "Escape", code: "Escape" }), ui({ renameOpen: true })),
    null,
  );
});

test("Escape closes the emoji picker after rename and before new-local", () => {
  const escape = key({ key: "Escape", code: "Escape" });
  assert.equal(
    resolveChatShortcut(escape, ui({ renameOpen: true, emojiPickerOpen: true })),
    null,
  );
  assert.equal(
    resolveChatShortcut(escape, ui({ settingsOpen: true, emojiPickerOpen: true })),
    "close-settings",
  );
  assert.equal(
    resolveChatShortcut(escape, ui({
      emojiPickerOpen: true,
      newLocalOpen: true,
      generating: true,
    })),
    "close-emoji-picker",
  );
});

test("Escape closes the new-local form before stopping generation", () => {
  assert.equal(
    resolveChatShortcut(key({ key: "Escape", code: "Escape" }), ui({
      newLocalOpen: true,
      generating: true,
    })),
    "close-new-local",
  );
});

test("Escape stops generation, then closes sources, then focuses the composer", () => {
  const escape = key({ key: "Escape", code: "Escape" });
  assert.equal(resolveChatShortcut(escape, ui({ generating: true, sourcesOpen: true })), "stop-generation");
  assert.equal(resolveChatShortcut(escape, ui({ sourcesOpen: true })), "close-sources");
  assert.equal(resolveChatShortcut(escape, ui()), "focus-composer");
});

test("unrelated keys do nothing", () => {
  assert.equal(resolveChatShortcut(key({ key: "a", code: "KeyA" }), ui()), null);
  assert.equal(resolveChatShortcut(key({ key: "Enter", code: "Enter", ctrlKey: true }), ui()), null);
});

test("isMacPlatform detects Apple platforms", () => {
  assert.equal(isMacPlatform("MacIntel"), true);
  assert.equal(isMacPlatform("iPhone"), true);
  assert.equal(isMacPlatform("Win32"), false);
});

test("filterCharacterChoices drops errors and matches name or id", () => {
  const entries = [
    { name: "Lyra", character_id: "lyra", error: null },
    { name: "Mina", character_id: "mina", error: null },
    { name: "Broken", character_id: "broken", error: "missing vault" },
  ];
  assert.deepEqual(filterCharacterChoices(entries, "").map((entry) => entry.character_id), ["lyra", "mina"]);
  assert.deepEqual(filterCharacterChoices(entries, "MIN").map((entry) => entry.character_id), ["mina"]);
  assert.deepEqual(filterCharacterChoices(entries, "lyra").map((entry) => entry.character_id), ["lyra"]);
  assert.deepEqual(filterCharacterChoices(entries, "broken"), []);
});

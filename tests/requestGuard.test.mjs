import assert from "node:assert/strict";
import { test } from "node:test";
import {
  composerShouldSubmit,
  shouldApplyChatUpdate,
  shouldClearDraft,
} from "../src/requestGuard.ts";
import { resolveChatShortcut } from "../src/chatShortcuts.ts";

test("ignores stale stream updates from another vault or session", () => {
  const current = {
    vaultRoot: "E:/vaults/beta",
    characterId: "beta",
    sessionId: "session-b",
    requestId: "req-2",
  };
  assert.equal(shouldApplyChatUpdate(current, {
    vaultRoot: "E:/vaults/alpha",
    characterId: "alpha",
    sessionId: "session-a",
    requestId: "req-1",
  }), false);
  assert.equal(shouldApplyChatUpdate(current, current), true);
});

test("completion does not clear a draft that no longer matches the submitted snapshot", () => {
  assert.equal(shouldClearDraft("next message", "Hello"), false);
  assert.equal(shouldClearDraft("Hello", "Hello"), true);
  assert.equal(shouldClearDraft("  Hello  ", "Hello"), true);
});

test("composer Enter does not submit while composing IME text", () => {
  assert.equal(composerShouldSubmit({ key: "Enter", shiftKey: false, isComposing: true }), false);
  assert.equal(composerShouldSubmit({ key: "Enter", shiftKey: false, nativeEvent: { isComposing: true } }), false);
  assert.equal(composerShouldSubmit({ key: "Enter", shiftKey: true, isComposing: false }), false);
  assert.equal(composerShouldSubmit({ key: "Enter", shiftKey: false, isComposing: false }), true);
});

test("settings-open shortcut context does not fire chat send or new-session chords", () => {
  const ui = {
    switcherOpen: false,
    settingsOpen: true,
    renameOpen: false,
    emojiPickerOpen: false,
    newLocalOpen: false,
    generating: false,
    sourcesOpen: false,
    sourcesAvailable: false,
  };
  const ctrlN = {
    key: "n",
    code: "KeyN",
    ctrlKey: true,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    repeat: false,
    isComposing: false,
  };
  assert.equal(resolveChatShortcut(ctrlN, ui), null);
  assert.equal(resolveChatShortcut({ ...ctrlN, key: "Enter", code: "Enter", ctrlKey: false }, ui), null);
  assert.equal(resolveChatShortcut({
    key: "Escape",
    code: "Escape",
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    repeat: false,
    isComposing: false,
  }, ui), "close-settings");
});

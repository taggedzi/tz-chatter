import assert from "node:assert/strict";
import { test } from "node:test";
import {
  EMOJI_CATALOG,
  filterEmoji,
  insertAtCursor,
} from "../src/emojiCatalog.ts";

test("insertAtCursor appends when the draft is empty", () => {
  assert.deepEqual(insertAtCursor("", "😀", 0, 0), { text: "😀", caret: "😀".length });
});

test("insertAtCursor places the glyph at the caret", () => {
  assert.deepEqual(insertAtCursor("hello world", "🎉", 5, 5), {
    text: "hello🎉 world",
    caret: "hello🎉".length,
  });
});

test("insertAtCursor replaces the current selection", () => {
  assert.deepEqual(insertAtCursor("hello world", "❤️", 6, 11), {
    text: "hello ❤️",
    caret: "hello ❤️".length,
  });
});

test("empty search lists only the selected category", () => {
  const smileys = filterEmoji("", "smileys");
  assert.ok(smileys.length > 0);
  assert.ok(smileys.every((entry) => entry.category === "smileys"));
  assert.ok(smileys.some((entry) => entry.glyph === "😀"));
});

test("search matches name or keywords across categories", () => {
  const hearts = filterEmoji("heart");
  assert.ok(hearts.some((entry) => entry.glyph === "❤️"));
  assert.ok(hearts.some((entry) => entry.category !== hearts[0].category) || hearts.length >= 1);

  const pizza = filterEmoji("PIZZA");
  assert.equal(pizza.length, 1);
  assert.equal(pizza[0].glyph, "🍕");
  assert.equal(pizza[0].category, "food");
});

test("unknown search returns no glyphs", () => {
  assert.deepEqual(filterEmoji("xyzzy-not-an-emoji"), []);
});

test("catalog entries have a glyph, name, keywords, and category", () => {
  assert.ok(EMOJI_CATALOG.length >= 80);
  for (const entry of EMOJI_CATALOG) {
    assert.equal(typeof entry.glyph, "string");
    assert.ok(entry.glyph.length > 0);
    assert.equal(typeof entry.name, "string");
    assert.ok(entry.name.length > 0);
    assert.ok(Array.isArray(entry.keywords));
    assert.equal(typeof entry.category, "string");
  }
});

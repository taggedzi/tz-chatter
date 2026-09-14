import test from "node:test";
import assert from "node:assert/strict";
import { selectedFolderValue } from "../src/folderPath.ts";
import { initiativeAvailability } from "../src/initiativeAvailability.ts";

test("folder selection preserves ordinary source folders", () => {
  assert.equal(selectedFolderValue("C:\\Characters\\Lyra"), "C:\\Characters\\Lyra");
});

test("new destination selection appends a child with the host separator", () => {
  assert.equal(
    selectedFolderValue("C:\\Exports\\", "tz-chatter-pack"),
    "C:\\Exports\\tz-chatter-pack",
  );
  assert.equal(
    selectedFolderValue("/home/me/exports/", "tz-chatter-pack"),
    "/home/me/exports/tz-chatter-pack",
  );
});

test("initiative controls require a character and enabling requires a session", () => {
  assert.deepEqual(initiativeAvailability("", "", "", false), {
    characterUnavailable: true,
    enableUnavailable: true,
    saveUnavailable: true,
  });
  assert.deepEqual(initiativeAvailability("vault", "lyra", "", false), {
    characterUnavailable: false,
    enableUnavailable: true,
    saveUnavailable: false,
  });
  assert.deepEqual(initiativeAvailability("vault", "lyra", "", true), {
    characterUnavailable: false,
    enableUnavailable: true,
    saveUnavailable: true,
  });
  assert.deepEqual(initiativeAvailability("vault", "lyra", "session", true), {
    characterUnavailable: false,
    enableUnavailable: false,
    saveUnavailable: false,
  });
});

import assert from "node:assert/strict";
import { test } from "node:test";
import { wrapTab } from "../src/settingsFocus.ts";

test("wrapTab ignores non-Tab keys", () => {
  const event = {
    key: "Enter",
    shiftKey: false,
    preventDefault() {
      throw new Error("should not prevent default");
    },
  };
  assert.equal(wrapTab({ querySelectorAll() { return []; } }, event), false);
});

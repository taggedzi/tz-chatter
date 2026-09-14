import test from "node:test";
import assert from "node:assert/strict";
import { parseGenerationFields } from "../src/characterEditor.ts";

test("character model fields preserve explicit values", () => {
  assert.deepEqual(parseGenerationFields("  llama3.2:latest  ", "0.7", "512"), {
    schema_version: 1,
    chat_model: "llama3.2:latest",
    temperature: 0.7,
    max_tokens: 512,
  });
});

test("empty character model fields inherit provider defaults", () => {
  assert.deepEqual(parseGenerationFields(null, "", ""), {
    schema_version: 1,
    chat_model: null,
    temperature: null,
    max_tokens: null,
  });
});

for (const value of ["-0.1", "2.1", "not-a-number"]) {
  test(`rejects invalid character temperature ${value} before save`, () => {
    assert.throws(
      () => parseGenerationFields(null, value, ""),
      /Temperature must be between 0 and 2/,
    );
  });
}

for (const value of ["15", "2049", "128.5", "not-a-number"]) {
  test(`rejects invalid character max tokens ${value} before save`, () => {
    assert.throws(
      () => parseGenerationFields(null, "", value),
      /Max tokens must be a whole number from 16 to 2048/,
    );
  });
}

import assert from "node:assert/strict";
import test from "node:test";

import {
  textsWithVersion,
  validateReleaseVersion,
  versionsFromTexts,
} from "../scripts/release-version.mjs";

function fixture(version = "0.1.0") {
  return {
    packageJson: JSON.stringify({ name: "tz-chatter", version }),
    packageLock: JSON.stringify({
      name: "tz-chatter",
      version,
      packages: { "": { name: "tz-chatter", version } },
    }),
    cargoToml: `[package]\nname = "tz-chatter"\nversion = "${version}"\n\n[dependencies]\nserde = "1"\n`,
    cargoLock: `version = 4\n\n[[package]]\nname = "tz-chatter"\nversion = "${version}"\ndependencies = []\n`,
    tauriConfig: JSON.stringify({ productName: "tz-chatter", version }),
  };
}

test("release versions accept stable and prerelease SemVer", () => {
  assert.equal(validateReleaseVersion("1.2.3"), "1.2.3");
  assert.equal(validateReleaseVersion("1.2.3-beta.4"), "1.2.3-beta.4");
});

test("release versions reject unsafe or ambiguous input", () => {
  for (const version of ["v1.2.3", "1.2", "1.2.3+local", "1.2.3/other", "$(whoami)"]) {
    assert.throws(() => validateReleaseVersion(version), /Invalid version/);
  }
});

test("version update synchronizes every release manifest", () => {
  const updated = textsWithVersion(fixture(), "0.2.0-beta.1");
  assert.deepEqual(versionsFromTexts(updated), {
    "package.json": "0.2.0-beta.1",
    "package-lock.json": "0.2.0-beta.1",
    "package-lock.json root package": "0.2.0-beta.1",
    "src-tauri/Cargo.toml": "0.2.0-beta.1",
    "src-tauri/Cargo.lock": "0.2.0-beta.1",
    "src-tauri/tauri.conf.json": "0.2.0-beta.1",
  });
});

test("Cargo replacement does not change dependency versions", () => {
  const updated = textsWithVersion(fixture(), "0.2.0");
  assert.match(updated.cargoToml, /serde = "1"/);
  assert.doesNotMatch(updated.cargoToml, /serde = "0\.2\.0"/);
});

test("version update preserves JSON line endings", () => {
  const texts = fixture();
  texts.packageJson = `${JSON.stringify({ name: "tz-chatter", version: "0.1.0" }, null, 2).replaceAll("\n", "\r\n")}\r\n`;
  const updated = textsWithVersion(texts, "0.2.0");
  assert.match(updated.packageJson, /\r\n/);
  assert.equal(updated.packageJson.replaceAll("\r\n", "").includes("\n"), false);
});

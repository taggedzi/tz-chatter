import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import { URL } from "node:url";
import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

const vendorRoot = new URL("../vendor/glib-0.18.5/", import.meta.url);
const provenanceUrl = new URL("../vendor/glib-0.18.5.upstream.json", import.meta.url);

test("vendored glib matches upstream with only the approved pointer fix", async () => {
  const provenance = JSON.parse(await readFile(provenanceUrl, "utf8"));
  const files = await readdir(vendorRoot, { recursive: true, withFileTypes: true });
  assert.equal(files.filter((entry) => entry.isFile()).length, Object.keys(provenance.files).length);
  for (const [path, expected] of Object.entries(provenance.files)) {
    let bytes = await readFile(new URL(path, vendorRoot));
    if (path === "src/variant_iter.rs") {
      const source = bytes.toString("utf8");
      assert.equal(source.split("let mut p: *mut libc::c_char = std::ptr::null_mut();").length, 2);
      assert.equal(source.split("                &mut p,").length, 2);
      bytes = Buffer.from(source
        .replace("let mut p: *mut libc::c_char", "let p: *mut libc::c_char")
        .replace("                &mut p,", "                &p,"));
    }
    assert.equal(createHash("sha256").update(bytes).digest("hex"), expected, path);
  }
});

test("application and regression share the glib patch and lockfile", async () => {
  const manifest = await readFile(new URL("../src-tauri/Cargo.toml", import.meta.url), "utf8");
  assert.match(manifest, /\[patch\.crates-io\][\s\S]*glib = \{ path = "\.\.\/vendor\/glib-0\.18\.5" \}/);
  assert.match(manifest, /members = \["security-tests\/glib-variant"\]/);
  const lock = await readFile(new URL("../src-tauri/Cargo.lock", import.meta.url), "utf8");
  const glibEntries = lock.split("[[package]]").filter((entry) => /^\s*name = "glib"$/m.test(entry));
  assert.equal(glibEntries.length, 1);
  assert.match(glibEntries[0], /version = "0\.18\.5"/);
  assert.doesNotMatch(glibEntries[0], /^source =|^checksum =/m);
});

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { URL } from "node:url";
import { checkImmutableReleases, requireImmutableResponse } from "../scripts/check-immutable-releases.mjs";

const repository = "taggedzi/tz-chatter";
const token = "synthetic-secret";

test("only boolean enabled true permits publication", () => {
  for (const body of ['{"enabled":true}', '{"enabled":true,"enforced_by_owner":false}']) {
    assert.doesNotThrow(() => requireImmutableResponse(body));
  }
  assert.throws(() => requireImmutableResponse('{"enabled":false}'), /disabled/);
  for (const body of ['{}', 'null', '[]', 'true', '{"enabled":"true"}', '{"enabled":1}', '{"enforced_by_owner":true}', '{"enabled":null}']) {
    assert.throws(() => requireImmutableResponse(body), /boolean enabled: true/);
  }
  for (const body of ['', '<html>Bad Gateway</html>', '{']) {
    assert.throws(() => requireImmutableResponse(body), /malformed JSON/);
  }
});

test("successful HTTP with disabled setting is rejected by the request gate", () => {
  assert.throws(() => checkImmutableReleases({ repository, token,
    request: () => ({ status: 0, stdout: '{"enabled":false,"enforced_by_owner":false}' }),
  }), /disabled/);
});

test("administrative credential is limited to a GET request and kept out of arguments", () => {
  let calls = 0;
  checkImmutableReleases({ repository, token, request: (command, args, options) => {
    calls++;
    assert.equal(command, "gh");
    assert.deepEqual(args, ["api", "--method", "GET", `repos/${repository}/immutable-releases`, "--header", "Accept: application/vnd.github+json"]);
    assert.equal(options.env.GH_TOKEN, token);
    assert.equal(options.timeout, 30_000);
    assert.equal(options.encoding, "utf8");
    return { status: 0, stdout: '{"enabled":true}' };
  } });
  assert.equal(calls, 1);
});

test("API, authorization, timeout and launch errors fail closed without leaking output", () => {
  for (const result of [
    { status: 1, stdout: '{"enabled":true}', stderr: `HTTP 403 ${token}` },
    { status: 1, stdout: '{"message":"Bad credentials"}', stderr: 'HTTP 401' },
    { status: 1, stdout: '', stderr: 'HTTP 404' },
    { status: null, error: new Error(`ETIMEDOUT ${token}`) },
    { status: null, error: new Error('ENOENT') },
  ]) {
    assert.throws(() => checkImmutableReleases({ repository, token, request: () => result }), error => {
      assert.match(error.message, /API request failed.*Administration: read/);
      assert.ok(!error.message.includes(token));
      return true;
    });
  }
});

test("missing credentials and invalid repository fail before requesting", () => {
  const request = () => assert.fail("must not request");
  for (const missing of [undefined, '', '  ']) {
    assert.throws(() => checkImmutableReleases({ repository, token: missing, request }), /Configure RELEASE_ADMIN_READ_TOKEN/);
  }
  for (const invalid of [undefined, '', '../bad', 'owner/repo/extra']) {
    assert.throws(() => checkImmutableReleases({ repository: invalid, token, request }), /GITHUB_REPOSITORY/);
  }
});

test("workflow gates publishing and offers isolated read-only verification", () => {
  const workflow = readFileSync(new URL('../.github/workflows/release.yml', import.meta.url), 'utf8');
  const immutability = workflow.split('  immutability:\n')[1].split('  validate:\n')[0];
  const publish = workflow.split('  publish:\n')[1];
  assert.match(immutability, /if: inputs.mode != 'dry-run'/);
  assert.match(immutability, /contents: read/);
  assert.match(immutability, /RELEASE_ADMIN_READ_TOKEN: \$\{\{ secrets.RELEASE_ADMIN_READ_TOKEN \}\}/);
  assert.match(immutability, /run: node scripts\/check-immutable-releases.mjs/);
  assert.doesNotMatch(immutability, /gh release|contents: write/);
  assert.match(workflow, /name: Validate release source\n {4}if: inputs.mode != 'check-immutability'/);
  assert.match(publish, /if: inputs.mode == 'prerelease' \|\| inputs.mode == 'stable'/);
  assert.match(publish, /needs: \[assemble, immutability\]/);
  assert.ok(publish.indexOf('run: node scripts/check-immutable-releases.mjs') < publish.indexOf('gh release create'));
  assert.ok(publish.indexOf('actions/checkout@') < publish.indexOf('run: node scripts/check-immutable-releases.mjs'));
  assert.doesNotMatch(workflow, /immutable-releases.*--silent/);
});

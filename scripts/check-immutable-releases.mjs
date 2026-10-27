import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import process from "node:process";

export function requireImmutableResponse(text) {
  let response;
  try {
    response = JSON.parse(text);
  } catch {
    throw new Error("Immutable-release API returned malformed JSON; publication is blocked.");
  }
  if (response?.enabled === false) {
    throw new Error("Immutable releases are disabled. Enable Settings -> General -> Releases -> Immutable releases before publishing.");
  }
  if (response?.enabled !== true) {
    throw new Error("Immutable-release API did not return boolean enabled: true; publication is blocked.");
  }
}

export function checkImmutableReleases({ repository, token, request = spawnSync }) {
  if (!/^[A-Za-z0-9][A-Za-z0-9_.-]*\/[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(repository ?? "")) {
    throw new Error("Set GITHUB_REPOSITORY to owner/repository; publication is blocked.");
  }
  if (!token?.trim()) {
    throw new Error("Configure RELEASE_ADMIN_READ_TOKEN with Administration: read for this repository; publication is blocked.");
  }
  const result = request("gh", [
    "api", "--method", "GET", `repos/${repository}/immutable-releases`,
    "--header", "Accept: application/vnd.github+json",
  ], { encoding: "utf8", timeout: 30_000, env: { ...process.env, GH_TOKEN: token } });
  if (result.error || result.status !== 0) {
    // Do not echo API bodies, stderr, or credentials into workflow logs.
    throw new Error("Immutable-release API request failed (network, HTTP, or authentication error). Check RELEASE_ADMIN_READ_TOKEN expiry, repository access and Administration: read permission; publication is blocked.");
  }
  requireImmutableResponse(result.stdout);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    checkImmutableReleases({ repository: process.env.GITHUB_REPOSITORY, token: process.env.RELEASE_ADMIN_READ_TOKEN });
    process.stdout.write("Immutable releases are explicitly enabled. Administrative read succeeded.\n");
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}

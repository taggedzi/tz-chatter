import { readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const manifestPaths = {
  packageJson: "package.json",
  packageLock: "package-lock.json",
  cargoToml: "src-tauri/Cargo.toml",
  cargoLock: "src-tauri/Cargo.lock",
  tauriConfig: "src-tauri/tauri.conf.json",
};

export function validateReleaseVersion(version) {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/.test(version ?? "")) {
    throw new Error(
      `Invalid version "${version ?? ""}". Use SemVer such as 0.2.0 or 0.2.0-beta.1.`,
    );
  }
  return version;
}

function packageVersion(cargoToml) {
  const match = cargoToml.match(/^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
  if (!match) throw new Error("Could not find [package].version in src-tauri/Cargo.toml.");
  return match[1];
}

function lockPackageVersion(cargoLock) {
  const match = cargoLock.match(
    /^\[\[package\]\]\r?\nname = "tz-chatter"\r?\nversion = "([^"]+)"/m,
  );
  if (!match) throw new Error("Could not find the tz-chatter package in src-tauri/Cargo.lock.");
  return match[1];
}

export function versionsFromTexts(texts) {
  const packageJson = JSON.parse(texts.packageJson);
  const packageLock = JSON.parse(texts.packageLock);
  const tauriConfig = JSON.parse(texts.tauriConfig);
  return {
    "package.json": packageJson.version,
    "package-lock.json": packageLock.version,
    "package-lock.json root package": packageLock.packages?.[""]?.version,
    "src-tauri/Cargo.toml": packageVersion(texts.cargoToml),
    "src-tauri/Cargo.lock": lockPackageVersion(texts.cargoLock),
    "src-tauri/tauri.conf.json": tauriConfig.version,
  };
}

function replaceCargoPackageVersion(cargoToml, version) {
  const pattern = /(^\[package\][\s\S]*?^version\s*=\s*")[^"]+("\s*$)/m;
  if (!pattern.test(cargoToml)) {
    throw new Error("Could not update [package].version in src-tauri/Cargo.toml.");
  }
  return cargoToml.replace(pattern, `$1${version}$2`);
}

function replaceCargoLockVersion(cargoLock, version) {
  const pattern = /(^\[\[package\]\]\r?\nname = "tz-chatter"\r?\nversion = ")[^"]+("\s*$)/m;
  if (!pattern.test(cargoLock)) {
    throw new Error("Could not update the tz-chatter package in src-tauri/Cargo.lock.");
  }
  return cargoLock.replace(pattern, `$1${version}$2`);
}

export function textsWithVersion(texts, version) {
  validateReleaseVersion(version);
  const packageJson = JSON.parse(texts.packageJson);
  const packageLock = JSON.parse(texts.packageLock);
  const tauriConfig = JSON.parse(texts.tauriConfig);

  packageJson.version = version;
  packageLock.version = version;
  if (!packageLock.packages?.[""]) {
    throw new Error("package-lock.json does not contain a root package entry.");
  }
  packageLock.packages[""].version = version;
  tauriConfig.version = version;

  const formatJson = (value, original) => {
    const eol = original.includes("\r\n") ? "\r\n" : "\n";
    return `${JSON.stringify(value, null, 2).replaceAll("\n", eol)}${eol}`;
  };

  return {
    packageJson: formatJson(packageJson, texts.packageJson),
    packageLock: formatJson(packageLock, texts.packageLock),
    cargoToml: replaceCargoPackageVersion(texts.cargoToml, version),
    cargoLock: replaceCargoLockVersion(texts.cargoLock, version),
    tauriConfig: formatJson(tauriConfig, texts.tauriConfig),
  };
}

async function readTexts(root) {
  return Object.fromEntries(
    await Promise.all(
      Object.entries(manifestPaths).map(async ([key, relativePath]) => [
        key,
        await readFile(path.join(root, relativePath), "utf8"),
      ]),
    ),
  );
}

function assertExpectedVersions(versions, expected) {
  const mismatches = Object.entries(versions).filter(([, value]) => value !== expected);
  if (mismatches.length > 0) {
    const details = mismatches.map(([name, value]) => `  ${name}: ${value ?? "missing"}`).join("\n");
    throw new Error(`Version ${expected} is not synchronized:\n${details}`);
  }
}

async function atomicWrite(target, contents) {
  const temporary = `${target}.release-version.tmp`;
  await writeFile(temporary, contents, "utf8");
  await rename(temporary, target);
}

async function run() {
  const [mode, rawVersion] = process.argv.slice(2);
  const version = validateReleaseVersion(rawVersion);
  if (mode !== "check" && mode !== "set") {
    throw new Error("Usage: node scripts/release-version.mjs <check|set> <version>");
  }

  const texts = await readTexts(repositoryRoot);
  if (mode === "set") {
    const updated = textsWithVersion(texts, version);
    await Promise.all(
      Object.entries(manifestPaths).map(([key, relativePath]) =>
        atomicWrite(path.join(repositoryRoot, relativePath), updated[key]),
      ),
    );
  }

  const current = versionsFromTexts(await readTexts(repositoryRoot));
  assertExpectedVersions(current, version);
  process.stdout.write(`Release version ${version} is synchronized across all manifests.\n`);
}

const isMain = process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url;
if (isMain) {
  run().catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}

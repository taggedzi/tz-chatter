/* global window, localStorage, setTimeout, WebSocket, fetch, console, URL, Buffer */
// T42 featured Holmes form/render review with synthetic IPC; not a native-dialog or Rust installer test.
// Requires Vite at http://127.0.0.1:1420/ and the installed Edge browser.
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";

const profile = await fs.mkdtemp(path.join(os.tmpdir(), "tz-gui-review-"));
const browser = spawn(
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  ["--headless=new", "--disable-gpu", "--no-first-run", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "about:blank"],
  { windowsHide: true, stdio: "ignore" },
);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let port;
for (let attempt = 0; attempt < 80; attempt += 1) {
  try {
    port = (await fs.readFile(path.join(profile, "DevToolsActivePort"), "utf8")).split("\n")[0];
    break;
  } catch {
    await sleep(100);
  }
}
if (!port) throw new Error("Browser debug endpoint unavailable");

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const socket = new WebSocket(targets.find((target) => target.type === "page").webSocketDebuggerUrl);
await new Promise((resolve) => socket.addEventListener("open", resolve, { once: true }));
let sequence = 0;
const pending = new Map();
const runtimeErrors = [];
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (!message.id) {
    if (message.method === "Runtime.exceptionThrown" || (message.method === "Runtime.consoleAPICalled" && message.params?.type === "error")) {
      runtimeErrors.push(message);
    }
    return;
  }
  const request = pending.get(message.id);
  pending.delete(message.id);
  if (message.error) request.reject(message.error);
  else request.resolve(message.result);
});
const call = (method, params = {}) => new Promise((resolve, reject) => {
  const id = ++sequence;
  pending.set(id, { resolve, reject });
  socket.send(JSON.stringify({ id, method, params }));
});
const evaluate = async (expression) => {
  const result = await call("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
};

const initialize = () => {
  const character = {
    schema_version: 1,
    id: "alpha",
    name: "Alpha",
    summary: "A thoughtful planning companion",
    system_prompt: "Be thoughtful and concise.",
    traits: ["curious", "steady"],
    boundaries: ["Do not claim to be human."],
    tags: [],
  };
  const turn = (id, role, content) => ({ id, timestamp: new Date().toISOString(), role, status: "complete", content });
  const transcript = {
    schema_version: 1,
    character_id: "alpha",
    session_id: "session-alpha",
    title: "Planning the next release",
    created_at: "",
    updated_at: "",
    turns: [
      turn("welcome", "assistant", "Welcome back. What would you like to work through?"),
      turn("question", "user", "Can we plan the next release together?"),
      turn("answer", "assistant", "Of course. Let’s start with the user journey and the remaining evidence."),
    ],
  };
  const provider = {
    id: "review",
    kind: "ollama",
    endpoint: "http://127.0.0.1:11434",
    chat_model: "llama3.2:latest",
    embedding_model: "nomic-embed-text",
    bearer_token: null,
  };
  let callbackId = 0;
  const callbacks = new Map();
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  window.reviewCalls = [];
  window.__TAURI_INTERNALS__ = {
    transformCallback: (callback) => {
      callbackId += 1;
      callbacks.set(callbackId, callback);
      return callbackId;
    },
    unregisterCallback: (id) => callbacks.delete(id),
    invoke: async (command, args = {}) => {
      window.reviewCalls.push({ command, args });
      if (command.startsWith("plugin:")) return null;
      if (command === "app_info") return { name: "tz-chatter", version: "0.1.0", stage: "review" };
      if (command === "load_provider_settings") return { schema_version: 1, active_provider_id: "review", providers: [provider] };
      if (command === "provider_discover") return { provider_id: "review", models: [{ id: "llama3.2:latest", supports_chat: true, supports_embeddings: false }] };
      if (command === "character_library_list") return { last_parent_dir: "C:\\Characters", entries: [{ vault_root: "alpha", character_id: "alpha", name: "Alpha", error: null, has_portrait: false }] };
      if (command === "character_load") return character;
      if (command === "character_portrait_load") return null;
      if (command === "persona_load") return { schema_version: 1, body: "A local-first software builder." };
      if (command === "generation_load") return { schema_version: 1, chat_model: null, temperature: null, max_tokens: null };
      if (command === "locals_list") return [{ id: "cafe", title: "Evening cafe" }];
      if (command === "local_load") return { schema_version: 1, id: "cafe", title: "Evening cafe", body: "A quiet corner table." };
      if (command === "local_save") return args.record;
      if (command === "scene_settings_load") return { schema_version: 1, default_local: "cafe" };
      if (command === "conversation_resume") return { character, transcript, transcript_fingerprint: "review" };
      if (command === "conversation_list_sessions") return [{ session_id: "session-alpha", title: transcript.title, created_at: "", updated_at: "", turn_count: 3, preview: "Can we plan the next release together?", archived: false }];
      if (command === "memory_browse" || command === "memory_review_queue" || command === "memory_conflict_pairs") return [];
      if (command === "initiative_snapshot") return {
        settings: { schema_version: 1, enabled: false, notifications_enabled: false, min_inactive_seconds: 900, cooldown_seconds: 1800, max_per_day: 3, max_ignored: 3, quiet_start_minute: 1320, quiet_end_minute: 480 },
        state: { sent_today: 0, ignored_streak: 0 },
        decision: { eligible: false, reasons: ["disabled"] },
      };
      return null;
    },
  };
  const originalInvoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = async (command, args) => {
    if (command === "character_install_holmes") {
      window.reviewCalls.push({ command, args });
      throw new Error("A sherlock-holmes folder already exists here. Choose a different parent folder.");
    }
    return originalInvoke(command, args);
  };
  localStorage.setItem("tz-chatter.vault-root", "alpha");
  localStorage.setItem("tz-chatter.active-character-id", "alpha");
  localStorage.setItem("tz-chatter.active-character-name", "Alpha");
  localStorage.setItem("tz-chatter.active-session-id", "session-alpha");
};

await call("Page.enable");
await call("Runtime.enable");
await call("Page.addScriptToEvaluateOnNewDocument", { source: `(${initialize.toString()})()` });
await call("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
await call("Page.navigate", { url: "http://localhost:1420/" });
await sleep(1200);
const globalResult = await call("Runtime.evaluate", { expression: "globalThis" });
const globalObjectId = globalResult.result.objectId;
if (!globalObjectId) throw new Error("Browser global object unavailable");
const callPageFunction = async (functionDeclaration, values = []) => {
  const result = await call("Runtime.callFunctionOn", {
    functionDeclaration,
    objectId: globalObjectId,
    arguments: values.map((value) => ({ value })),
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
};

const screenshot = async (name) => {
  const result = await call("Page.captureScreenshot", { format: "png" });
  await fs.writeFile(new URL(name, import.meta.url), Buffer.from(result.data, "base64"));
};
const clickText = async (label) => {
  await callPageFunction(
    `function(label) {
      const button = [...document.querySelectorAll("button")].find((candidate) => candidate.textContent.trim().includes(label));
      if (!button) throw new Error("Button not found");
      button.click();
    }`,
    [label],
  );
  await sleep(180);
};

const results = { evidence: "Real React layout with synthetic IPC; actual installer separately covered by Rust tests." };
try {
  await clickText("Characters");
  for (const [width, height] of [[1280, 800], [900, 620]]) {
    await call("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
    await sleep(150);
    await evaluate('document.querySelector(".featured-character").scrollIntoView({block:"center"})');
    const layout = await evaluate('(() => { const card = document.querySelector(".featured-character"); const rect = card.getBoundingClientRect(); const img = card.querySelector("img"); return {left:rect.left, right:rect.right, width:rect.width, portraitLoaded:img.complete && img.naturalWidth > 0, viewportWidth:innerWidth, documentWidth:document.documentElement.scrollWidth}; })()');
    if (!layout.portraitLoaded || layout.left < 0 || layout.right > width || layout.documentWidth > width) throw new Error("Featured card layout failed: " + JSON.stringify(layout));
    results[width] = layout;
    await screenshot("holmes-featured-" + width + ".png");
  }
  await clickText("Meet Sherlock Holmes");
  results.parentFolderField = await evaluate('Boolean(document.querySelector(".featured-character .folder-field input"))');
  results.browseButton = await evaluate('Boolean(document.querySelector(".featured-character .folder-field button"))');
  await clickText("Create my Holmes");
  results.installCall = await evaluate('window.reviewCalls.find((entry) => entry.command === "character_install_holmes")');
  results.conflictVisible = await evaluate('document.querySelector(".character-feedback[role=alert]")?.textContent.includes("already exists")');
  results.formRecovered = await evaluate('!document.querySelector(".featured-character button[type=submit]").disabled');
  if (!results.parentFolderField || !results.browseButton || !results.installCall?.args.parentDir || !results.conflictVisible || !results.formRecovered) throw new Error("Installer UI assertions failed");
  await evaluate('document.querySelector(".featured-character").scrollIntoView({block:"center"})');
  await screenshot("holmes-install-conflict-900.png");
  results.runtimeErrorCount = runtimeErrors.length;
  if (runtimeErrors.length) throw new Error("Unexpected browser runtime errors");
  await fs.writeFile(new URL("holmes-ui-results.json", import.meta.url), JSON.stringify(results, null, 2));
  console.log(JSON.stringify(results, null, 2));
} finally {
  await call("Browser.close").catch(() => {});
  socket.close();
  browser.unref();
}

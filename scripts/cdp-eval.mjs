// Evaluate one expression in the running Romp WebView2 (dev builds with
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222).
// Usage: node scripts/cdp-eval.mjs "await window.omp.tabs.list()"
import { readFileSync } from "node:fs";
const raw = process.argv[2] ?? "1 + 1";
// `@path` reads the expression from a UTF-8 file so native argv quoting cannot
// mangle backslashes or embedded quotes (PowerShell 5.1 strips them).
const expression = raw.startsWith("@") ? readFileSync(raw.slice(1), "utf8") : raw;
const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
// Prefer the dev server, then the installed app's tauri origin, then any other
// non-http page. Matching tauri.localhost explicitly matters: the installed
// build serves http://tauri.localhost/, which the old !startsWith("http") test
// rejected, silently falling through to "first page" (wrong when the Browser
// panel adds a second target).
const target =
  targets.find(item => item.type === "page" && item.url.includes("localhost:1420")) ??
  targets.find(item => item.type === "page" && item.url.includes("tauri.localhost")) ??
  targets.find(item => item.type === "page" && !item.url.startsWith("http")) ??
  targets.find(item => item.type === "page");
if (!target) {
  console.error("no page target");
  process.exit(1);
}
const socket = new WebSocket(target.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", event => {
  const message = JSON.parse(event.data);
  const resolve = pending.get(message.id);
  if (resolve) {
    pending.delete(message.id);
    resolve(message);
  }
});
const send = (method, params) =>
  new Promise(resolve => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });
socket.addEventListener("open", async () => {
  const response = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
    replMode: true,
  });
  const payload = response.exceptionDetails ? response : (response.result ?? response.error);
  console.log(JSON.stringify(payload, null, 1));
  socket.close();
});

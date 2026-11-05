// Send one CDP command to the running Romp WebView2.
// Usage: node scripts/cdp-cmd.mjs Runtime.evaluate '{"expression":"1+1","returnByValue":true}'
const method = process.argv[2] ?? "Runtime.evaluate";
const params = JSON.parse(process.argv[3] ?? "{}");
const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
// A tab with the Browser panel open has a second page target; drive the app one.
const target =
  targets.find(item => item.type === "page" && item.url.includes("localhost:1420")) ??
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
socket.addEventListener("open", () => {
  const id = nextId++;
  pending.set(id, message => {
    console.log(JSON.stringify(message.result ?? message.error, null, 1));
    socket.close();
  });
  socket.send(JSON.stringify({ id, method, params }));
});

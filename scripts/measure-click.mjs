// Measure sidebar click -> first transcript bubble using the app's own DOM signals.
// Usage: node scripts/measure-click.mjs "session title"
const title = process.argv[2];
if (!title) {
  console.error("usage: node scripts/measure-click.mjs <sidebar title>");
  process.exit(1);
}
const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const target = targets.find(item => item.type === "page");
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
    resolve(message.result?.result?.value ?? message.result ?? message.error);
  }
});
const evaluate = expression =>
  new Promise(resolve => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({
      id,
      method: "Runtime.evaluate",
      params: { expression, awaitPromise: true, returnByValue: true, replMode: true },
    }));
  });
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

socket.addEventListener("open", async () => {
  const clicked = await evaluate(`
    (() => {
      const title = ${JSON.stringify(title)};
      const row = [...document.querySelectorAll('.omp-sidebar-title')]
        .find(node => node.textContent.trim() === title);
      if (!row) return 'missing';
      window.__title = title;
      window.__t0 = performance.now();
      window.__bubbles0 = document.querySelectorAll('.omp-user-bubble-content').length;
      row.click();
      return 'clicked';
    })()
  `);
  if (clicked === "missing") {
    console.log(JSON.stringify({ error: "sidebar row not found" }));
    socket.close();
    return;
  }
  const started = Date.now();
  let result = null;
  while (Date.now() - started < 30000) {
    await sleep(100);
    const snapshot = await evaluate(`
      (() => ({
        ms: Math.round(performance.now() - window.__t0),
        target: [...document.querySelectorAll('.omp-user-bubble-content')]
          .some(node => node.textContent.includes(window.__title)),
      }))()
    `);
    if (snapshot && snapshot.target && !snapshot.pending) {
      result = { title, clickToFirstBubbleMs: snapshot.ms };
      break;
    }
  }
  const tabs = await evaluate(`await window.omp.tabs.list().then(list =>
    list.map(tab => ({ id: tab.tabId, active: tab.active, status: tab.status })))`);
  console.log(JSON.stringify({ result: result ?? { title, timeout: true }, tabs }, null, 1));
  socket.close();
});

// A smoke test of the running app, without keyboard or mouse input: it sends
// the sidebar's own commands through the DevTools protocol and checks the
// state that comes back. It finds hangs and broken basics in about two
// minutes; it is no substitute for the unit tests.
//
//   MAGNETITE_DEVTOOLS_PORT=9333 <magnetite> --no-import --mock-keychain \
//       --data-dir <a throw-away folder> &
//   node tools/smoke.mjs            # exit code 0: all checks passed
//
// Needs Node 22 or later and a development build (release builds ignore the
// port). The pages it opens are on example.org.
const base = `http://127.0.0.1:${process.env.MAGNETITE_DEVTOOLS_PORT ?? 9333}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const targets = async () => (await fetch(`${base}/json`)).json();

/** A DevTools session with one page. */
async function attach(target) {
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve);
    ws.addEventListener("error", reject);
  });
  let seq = 0;
  const pending = new Map();
  ws.addEventListener("message", (m) => {
    const d = JSON.parse(m.data);
    if (d.id && pending.has(d.id)) {
      pending.get(d.id)(d);
      pending.delete(d.id);
    }
  });
  const cdp = (method, params = {}) =>
    new Promise((resolve) => {
      const id = ++seq;
      pending.set(id, resolve);
      ws.send(JSON.stringify({ id, method, params }));
    });
  const evaluate = async (expression, userGesture = false) => {
    const d = await cdp("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true, userGesture });
    if (d.result?.exceptionDetails) throw new Error(d.result.exceptionDetails.exception?.description ?? d.result.exceptionDetails.text);
    return d.result?.result?.value;
  };
  return { evaluate, close: () => ws.close() };
}

let sidebarTarget;
for (let i = 0; i < 60 && !sidebarTarget; i++) {
  try {
    sidebarTarget = (await targets()).find((t) => t.type === "page" && t.url.startsWith("magnetite://ui/") && !/command|notifications|float|settings/.test(t.url));
  } catch {
    // Not listening yet.
  }
  if (!sidebarTarget) await sleep(500);
}
if (!sidebarTarget) {
  console.log(`FAIL no sidebar on ${base}: is a development build running with MAGNETITE_DEVTOOLS_PORT?`);
  process.exit(1);
}
const ui = await attach(sidebarTarget);
// The page is listed before its script has run.
for (let i = 0; i < 40 && !(await ui.evaluate(`!!window.__magnetite?.setState`)); i++) await sleep(250);
// Keep the state the app last sent, to read it back.
await ui.evaluate(`(() => { const set = window.__magnetite.setState; window.__magnetite.setState = (s) => { window.__last = s; set(s); }; })()`);

/** Send a command as the sidebar does; a command without an answer within 8 s counts as a hang. */
const send = async (command) =>
  JSON.parse(
    await Promise.race([
      ui.evaluate(`new Promise((res, rej) => window.magnetiteQuery({ request: ${JSON.stringify(JSON.stringify(command))}, onSuccess: (r) => res(r || "{}"), onFailure: (c, m) => rej(new Error(m)) }))`),
      sleep(8000).then(() => {
        throw new Error(`no answer to ${command.cmd} (hang?)`);
      }),
    ]),
  );
const state = async () => JSON.parse(await ui.evaluate(`JSON.stringify(window.__last ?? null)`));
const space = (s) => s.sidebar.spaces.find((x) => x.id === s.active_space);
const has = (s, id) => JSON.stringify(s.sidebar).includes(`"${id}"`);
const results = [];
const check = (name, ok, detail = "") => {
  results.push(ok);
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
};
/** Open `url` as a tab and give its id. */
const open = async (url, background = false) => {
  await send({ cmd: "new_tab", url, background });
  await sleep(2500);
  return space(await state()).today[0].id;
};

const first = await open("https://example.org/");
const second = await open("https://example.org/?second", true);
let s = await state();
check("tabs open, the first one in front", s.active_tab === first && has(s, second));

await send({ cmd: "add_split", id: second });
await sleep(1500);
s = await state();
check("two pages share the screen", s.panes.length === 2 && s.split != null, `panes: ${s.panes.length}`);

// A link that asks for a new window in a pinned tab opens as a peek.
await send({ cmd: "pin", id: first });
await sleep(500);
const page = (await targets()).find((t) => t.type === "page" && t.url === "https://example.org/");
if (page) {
  const session = await attach(page);
  await session.evaluate(`window.open("https://example.org/?peek")`, true);
  session.close();
  await sleep(3000);
  s = await state();
  check("a peek opens from a pinned tab", s.peek != null);
  await send({ cmd: "shortcut", shortcut: "close_tab" });
  await sleep(1500);
  s = await state();
  check("the close key closes the peek and keeps the tab", s.peek == null && has(s, first));
} else check("found the page to open a peek from", false);

await send({ cmd: "new_folder", title: "Smoke" });
await sleep(500);
s = await state();
const folder = space(s).pinned.find((n) => n.kind === "folder");
await send({ cmd: "move_tab", id: second, space: s.active_space, section: "pinned", index: 0, folder: folder.id });
await send({ cmd: "delete_folder", id: folder.id });
await sleep(800);
s = await state();
check("a deleted folder leaves its tab", !space(s).pinned.some((n) => n.kind === "folder") && space(s).pinned.some((n) => n.id === second));

await send({ cmd: "move_tab", id: second, space: s.active_space, section: "favorites", index: 0 });
await sleep(500);
s = await state();
check("a tab becomes a favorite", s.sidebar.favorites.some((f) => f.items.some((t) => t.id === second)));
await send({ cmd: "move_tab", id: second, space: "no-such-space", section: "today", index: 0 });
await sleep(500);
check("a move to a space that is gone loses nothing", has(await state(), second));

const third = await open("https://example.org/?third");
await send({ cmd: "close_tab", id: third });
await sleep(800);
const archived = (await send({ cmd: "archive" })).items;
check("a closed tab is in the archive", archived.some((a) => a.id === third));
await send({ cmd: "remove_archived", id: third });
check("an archived tab can be removed", !(await send({ cmd: "archive" })).items.some((a) => a.id === third));

console.log("waiting for a minute tick …");
await sleep(70000);
await send({ cmd: "go_back" });
s = await state();
check("still answering after the tick, pages on screen kept", has(s, first) && has(s, second));

ui.close();
console.log(results.every(Boolean) ? "smoke: all ok" : "smoke: FAILED");
process.exit(results.every(Boolean) ? 0 : 1);

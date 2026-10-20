// Shared helpers for the browser tests (tools/test/browser/*.test.mjs): a static server for the
// built game, headless Chrome (DevTools protocol) and headless Firefox (WebDriver BiDi), and a
// small test harness. No dependencies: Node 22+ has WebSocket and fetch built in.
//
// Every browser gets a throwaway profile, a fixed 1280x720 page and no network beyond the local
// server, so a test sees the same page every time. Tests wait for the game's own log lines (for
// example "-> Game/Menu") and for page state, never for a fixed time, except for short settles
// after input.
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

export const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** The page size every test uses. Click positions in the tests are for this size. */
export const VIEW = { width: 1280, height: 720 };

/** The four game files, from UB_WINDATA or game-data/original/WinData; null when missing. */
export function gameFiles() {
  const dir = process.env.UB_WINDATA || join(repo, 'game-data/original/WinData');
  const files = ['data0.pkg', 'data1.pkg', 'patch0.pkg', 'patch1.pkg'].map((f) => join(dir, f));
  return files.every((f) => existsSync(f)) ? files : null;
}

/** A file in the repository's git-ignored folders, or null when it is missing. */
export function optionalFile(...parts) {
  const f = join(repo, ...parts);
  return existsSync(f) ? f : null;
}

// ---------------------------------------------------------------------------------------------
// Static server

const TYPES = {
  '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.wasm': 'application/wasm',
  '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png', '.webp': 'image/webp',
  '.json': 'application/json', '.webmanifest': 'application/manifest+json', '.ico': 'image/x-icon',
};

/** Serve `dir` on 127.0.0.1 at a free port. Extra `pages` map a path to HTML text. */
export async function serve(dir, pages = {}) {
  const server = createServer((req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (pages[path]) {
      res.writeHead(200, { 'content-type': 'text/html', 'cache-control': 'no-store' });
      return res.end(pages[path]);
    }
    let file = join(dir, path);
    if (!file.startsWith(dir)) { res.writeHead(403); return res.end(); }
    if (existsSync(file) && statSync(file).isDirectory()) file = join(file, 'index.html');
    if (!existsSync(file)) { res.writeHead(404); return res.end(); }
    res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
    res.end(readFileSync(file));
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const url = `http://127.0.0.1:${server.address().port}`;
  return { url, close: () => new Promise((r) => server.close(r)) };
}

// ---------------------------------------------------------------------------------------------
// Browsers

function chromePath() {
  if (process.env.CHROME) return process.env.CHROME;
  const mac = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  if (existsSync(mac)) return mac;
  for (const p of ['/usr/bin/google-chrome', '/usr/bin/chromium', '/usr/bin/chromium-browser']) if (existsSync(p)) return p;
  return null;
}

function firefoxPath() {
  if (process.env.FIREFOX) return process.env.FIREFOX;
  for (const p of ['/Applications/Firefox Nightly.app/Contents/MacOS/firefox', '/Applications/Firefox.app/Contents/MacOS/firefox', '/usr/bin/firefox']) {
    if (existsSync(p)) return p;
  }
  return null;
}

export const hasChrome = () => chromePath() !== null;
export const hasFirefox = () => firefoxPath() !== null;

/** How long one protocol call may take before the test fails. */
const CALL_TIMEOUT = 30000;
/** How long one test may take, unless it sets its own `timeout` (seconds). */
const TEST_TIMEOUT = 300;

function withTimeout(what, ms, promise) {
  let timer;
  const limit = new Promise((_, j) => { timer = setTimeout(() => j(new Error(`${what}: no answer within ${ms / 1000} s`)), ms); });
  return Promise.race([promise, limit]).finally(() => clearTimeout(timer));
}

async function until(what, f, timeout) {
  const end = Date.now() + timeout;
  for (;;) {
    const v = await f();
    if (v) return v;
    if (Date.now() > end) throw new Error(`timed out after ${timeout / 1000} s waiting for ${what}`);
    await sleep(100);
  }
}

/** Common browser methods; `impl` supplies the protocol-specific ones. */
class Browser {
  constructor() { this.logs = []; this.mark = 0; }

  /** Wait until a log line matching `re` appears after the last `markLogs()`. */
  async waitLog(re, timeout = 60000) {
    const from = this.mark;
    return until(`log ${re}`, () => this.logs.slice(from).find((l) => re.test(l)), timeout);
  }

  /** Remember the current end of the log, so `waitLog` sees only new lines. */
  markLogs() { this.mark = this.logs.length; }

  /** Wait until `expr` is truthy in the page. */
  async waitFor(expr, timeout = 30000) {
    return until(expr, async () => { try { return await this.eval(expr); } catch { return false; } }, timeout);
  }

  /**
   * Wait until the page has loaded and the game's code has started: only then does the start
   * page take files (`window.wasmBindings` is set once the WebAssembly module has run).
   */
  async waitReady(timeout = 60000) {
    await this.waitFor(`document.readyState === 'complete' && !!window.wasmBindings`, timeout);
  }

  /**
   * Run `action`, which makes the page reload (the start page's manager does after each
   * change), and wait until the new page's game code has started.
   */
  async expectReload(action, timeout = 30000) {
    await this.eval('window.__ubBefore = true');
    await action();
    await this.waitFor('!window.__ubBefore', timeout);
    await this.waitReady();
  }

  /** Choose the four game files (and any extra files) with the start page's file picker. */
  async chooseFiles(files) { await this.setFiles('#packages', files); }

  /** Choose files and wait until the main menu is open. */
  async startWithFiles(files, timeout = 120000) {
    await this.chooseFiles(files);
    await this.waitMenu(timeout);
  }

  /** Wait until the main menu's dialog is open (the game is running and the menu is shown). */
  async waitMenu(timeout = 120000) {
    await this.waitLoop('Game/Menu', timeout);
    await this.waitDialog('MainMenu');
  }

  /** Errors the game logged (panics and uncaught exceptions). */
  errors() { return this.logs.filter((l) => /^error /.test(l)); }

  /** Run a probe command in the game (apps/game/src/probe.rs) and return its answer. */
  async probe(command) {
    return this.eval(`window.unbanished.probe(${JSON.stringify(command)})`);
  }

  /** Click the first usable element matching a UI query, as a player would. */
  async press(query) { const r = await this.probe(`click ${query}`); await sleep(200); return r; }

  /**
   * Click an element that makes the page go away (Exit reloads, closes or removes the page):
   * send the click and do not wait for an answer that cannot come. `frame` is a JavaScript
   * expression for the window that holds the game, when it is not the page itself.
   */
  async pressAndLeave(query, frame = 'window') {
    await this.eval(`void ${frame}.unbanished.probe(${JSON.stringify(`click ${query}`)}).catch(() => {}), true`);
  }

  /** Wait until the game's current loop is `name` (`Game/Menu`, `Game/Game`). */
  async waitLoop(name, timeout = 120000) {
    return until(`the ${name} loop`, async () => {
      try { return (await this.probe('state')).loop === name; } catch { return false; }
    }, timeout);
  }

  /** Wait until a dialog whose reference contains `part` is open. */
  async waitDialog(part, timeout = 30000) {
    return until(`a ${part} dialog`, async () => {
      try { return (await this.probe('state')).dialogs.some((d) => d.includes(part)); } catch { return false; }
    }, timeout);
  }

  /** Start a new game from the main menu with a fixed seed and name, and wait until it runs. */
  async newGame(seed = 1234, name = 'Testwood') {
    await this.press('ref:MainMenu text:New');
    await this.waitDialog('NewGame');
    await this.probe(`type editName=${name}`);
    await this.probe(`type editSeed=${seed}`);
    await this.press('ref:NewGame buttonOk');
    await this.waitLoop('Game/Game');
  }
}

class Chrome extends Browser {
  static async launch({ standalone = false } = {}) {
    const path = chromePath();
    if (!path) throw new Error('Chrome not found (set CHROME)');
    const b = new Chrome();
    b.profile = mkdtempSync(join(tmpdir(), 'ub-test-chrome-'));
    b.proc = spawn(path, [
      '--headless=new', '--remote-debugging-port=0', `--user-data-dir=${b.profile}`,
      '--enable-unsafe-webgpu', '--enable-gpu', '--use-angle=metal', '--ignore-gpu-blocklist',
      '--no-first-run', '--no-default-browser-check', '--disable-background-timer-throttling',
      '--disable-renderer-backgrounding', `--window-size=${VIEW.width},${VIEW.height}`, 'about:blank',
    ], { stdio: 'ignore' });
    const portFile = join(b.profile, 'DevToolsActivePort');
    const port = await until('Chrome to start', () => existsSync(portFile) && readFileSync(portFile, 'utf8').split('\n')[0], 20000);
    const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
    b.port = port;
    b.ws = new WebSocket(version.webSocketDebuggerUrl);
    await new Promise((r, j) => { b.ws.onopen = r; b.ws.onerror = j; });
    b.id = 0; b.waiting = new Map();
    b.ws.onmessage = (m) => b.onMessage(JSON.parse(m.data));
    b.standalone = standalone;
    return b;
  }

  onMessage(d) {
    if (d.id && this.waiting.has(d.id)) { this.waiting.get(d.id)(d); this.waiting.delete(d.id); return; }
    if (d.method === 'Runtime.consoleAPICalled') {
      const text = d.params.args.map((a) => a.value ?? a.description ?? '').join(' ');
      this.logs.push(`${d.params.type === 'error' ? 'error' : d.params.type} ${text}`);
    } else if (d.method === 'Runtime.exceptionThrown') {
      this.logs.push(`error ${d.params.exceptionDetails.exception?.description ?? d.params.exceptionDetails.text}`);
    } else if (d.method === 'Target.targetDestroyed' && d.params.targetId === this.target) {
      this.closed = true;
    }
  }

  raw(method, params = {}, sessionId = this.session, timeout = CALL_TIMEOUT) {
    return withTimeout(method, timeout, new Promise((r, j) => {
      const i = ++this.id;
      this.waiting.set(i, (d) => (d.error ? j(new Error(`${method}: ${d.error.message}`)) : r(d.result)));
      this.ws.send(JSON.stringify({ id: i, method, params, ...(sessionId ? { sessionId } : {}) }));
    }));
  }

  /**
   * Open `url` as a new page. Its history has one entry, as in a freshly started installed app.
   * With `standalone`, the page reports `display-mode: standalone` (a matchMedia override:
   * headless Chrome cannot install an app).
   */
  async open(url) {
    await this.raw('Target.setDiscoverTargets', { discover: true }, null);
    // Created at its URL, the page's history has one entry, as in a new installed app window.
    const { targetId } = await this.raw('Target.createTarget', { url }, null);
    this.target = targetId;
    const { sessionId } = await this.raw('Target.attachToTarget', { targetId, flatten: true }, null);
    this.session = sessionId;
    await this.raw('Page.enable'); await this.raw('Runtime.enable'); await this.raw('DOM.enable');
    await this.raw('Emulation.setDeviceMetricsOverride', { ...VIEW, deviceScaleFactor: 1, mobile: false });
    await this.raw('Emulation.setFocusEmulationEnabled', { enabled: true });
    if (this.standalone) {
      await this.raw('Page.addScriptToEvaluateOnNewDocument', { source: `{
        const real = window.matchMedia.bind(window);
        window.matchMedia = (q) => /display-mode:\\s*standalone/.test(q)
          ? Object.defineProperty(real('all'), 'matches', { value: true }) : real(q);
      }` });
    }
    // Reload, so the page starts again with the override and the page size in place; a reload
    // adds no history entry.
    this.markLogs();
    await this.raw('Page.reload', {});
    await this.waitReady();
  }

  async eval(expression) {
    const r = await this.raw('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
    return r.result.value;
  }

  async setFiles(selector, files) {
    const { root } = await this.raw('DOM.getDocument', { depth: 1 });
    const { nodeId } = await this.raw('DOM.querySelector', { nodeId: root.nodeId, selector });
    await this.raw('DOM.setFileInputFiles', { nodeId, files });
  }

  async click(x, y) {
    for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) {
      await this.raw('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1 });
    }
    await sleep(300);
  }

  async key(key) {
    const codes = { Escape: 27, Enter: 13 };
    for (const type of ['keyDown', 'keyUp']) {
      await this.raw('Input.dispatchKeyEvent', { type, key, code: key, windowsVirtualKeyCode: codes[key] ?? 0 });
    }
    await sleep(300);
  }

  async screenshot(file) {
    const { data } = await this.raw('Page.captureScreenshot', { format: 'png' });
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, Buffer.from(data, 'base64'));
  }

  /** True once the page's window has been closed (by the page itself). */
  async waitClosed(timeout = 10000) { return until('the window to close', () => this.closed, timeout); }

  async close() {
    try { this.ws.close(); } catch {}
    this.proc.kill('SIGKILL');
    await sleep(200);
    rmSync(this.profile, { recursive: true, force: true });
  }
}

class Firefox extends Browser {
  static async launch({ webgpu = false } = {}) {
    const path = firefoxPath();
    if (!path) throw new Error('Firefox not found (set FIREFOX)');
    const b = new Firefox();
    b.profile = mkdtempSync(join(tmpdir(), 'ub-test-firefox-'));
    // Headless Firefox offers WebGPU but gives no WebGPU canvas, so the tests use WebGL 2.
    writeFileSync(join(b.profile, 'user.js'), webgpu ? '' : 'user_pref("dom.webgpu.enabled", false);\n');
    b.proc = spawn(path, ['--headless', '--remote-debugging-port=0', '--profile', b.profile, '--no-remote',
      `--width=${VIEW.width}`, `--height=${VIEW.height}`],
      // Setting the page size through BiDi needs this.
      { stdio: ['ignore', 'ignore', 'pipe'], env: { ...process.env, MOZ_REMOTE_ALLOW_SYSTEM_ACCESS: '1' } });
    let err = '';
    b.proc.stderr.on('data', (d) => { err += d; });
    const url = await until('Firefox to start', () => err.match(/WebDriver BiDi listening on (ws:\/\/\S+)/)?.[1], 30000);
    b.ws = new WebSocket(`${url}/session`);
    await new Promise((r, j) => { b.ws.onopen = r; b.ws.onerror = j; });
    b.id = 0; b.waiting = new Map();
    b.ws.onmessage = (m) => b.onMessage(JSON.parse(m.data));
    await b.raw('session.new', { capabilities: {} });
    await b.raw('session.subscribe', { events: ['log.entryAdded', 'browsingContext.contextDestroyed'] });
    // A tab of its own: the first context may be a privileged one that takes no page size.
    b.context = (await b.raw('browsingContext.create', { type: 'tab' })).context;
    await b.raw('browsingContext.setViewport', { context: b.context, viewport: VIEW });
    return b;
  }

  onMessage(d) {
    if (d.id && this.waiting.has(d.id)) { this.waiting.get(d.id)(d); this.waiting.delete(d.id); return; }
    if (d.method === 'log.entryAdded') this.logs.push(`${d.params.level} ${d.params.text}`);
    if (d.method === 'browsingContext.contextDestroyed' && d.params.context === this.context) this.closed = true;
  }

  raw(method, params = {}, timeout = CALL_TIMEOUT) {
    return withTimeout(method, timeout, new Promise((r, j) => {
      const i = ++this.id;
      this.waiting.set(i, (d) => (d.type === 'error' ? j(new Error(`${method}: ${d.error} ${d.message}`)) : r(d.result)));
      this.ws.send(JSON.stringify({ id: i, method, params }));
    }));
  }

  async open(url) {
    await this.raw('browsingContext.navigate', { context: this.context, url, wait: 'complete' });
    await this.waitReady();
  }

  async eval(expression) {
    // BiDi returns objects in its own wrapped form; JSON text comes back as it is.
    const wrapped = `(async () => { const v = await (${expression}); return v === undefined ? 'null' : JSON.stringify(v); })()`;
    const r = await this.raw('script.evaluate', { expression: wrapped, target: { context: this.context }, awaitPromise: true });
    if (r.type === 'exception') throw new Error(r.exceptionDetails.text);
    return JSON.parse(r.result.value ?? 'null') ?? undefined;
  }

  async setFiles(selector, files) {
    const r = await this.raw('script.evaluate', { expression: `document.querySelector(${JSON.stringify(selector)})`,
      target: { context: this.context }, awaitPromise: false, resultOwnership: 'root' });
    await this.raw('input.setFiles', { context: this.context, element: { sharedId: r.result.sharedId }, files });
  }

  async click(x, y) {
    await this.raw('input.performActions', { context: this.context, actions: [{ type: 'pointer', id: 'mouse', actions: [
      { type: 'pointerMove', x, y }, { type: 'pointerDown', button: 0 }, { type: 'pointerUp', button: 0 }] }] });
    await sleep(300);
  }

  async key(key) {
    const value = { Escape: '', Enter: '' }[key] ?? key;
    await this.raw('input.performActions', { context: this.context, actions: [{ type: 'key', id: 'keyboard', actions: [
      { type: 'keyDown', value }, { type: 'keyUp', value }] }] });
    await sleep(300);
  }

  async screenshot(file) {
    const { data } = await this.raw('browsingContext.captureScreenshot', { context: this.context });
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, Buffer.from(data, 'base64'));
  }

  async close() {
    try { this.ws.close(); } catch {}
    this.proc.kill('SIGKILL');
    await sleep(200);
    rmSync(this.profile, { recursive: true, force: true });
  }
}

export const chrome = (options) => Chrome.launch(options);
export const firefox = (options) => Firefox.launch(options);

// ---------------------------------------------------------------------------------------------
// Harness

/** Thrown by a test to skip itself, with the reason (for example a missing optional file). */
export class Skip extends Error {}

/** Fail with `message` unless `value` is truthy. */
export function check(value, message) {
  if (!value) throw new Error(message);
}

/** Fail unless `actual` equals `expected` (compared as JSON). */
export function equal(actual, expected, what) {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${what}: expected ${e}, got ${a}`);
}

/**
 * Run `tests` ({ name, needs, run }) one after another and print one line each. `needs` is a list
 * of requirements ('chrome', 'firefox', 'gamedata'); a test whose needs are missing is skipped
 * and says why. Exit code 1 when any test failed. Results go to $UB_TEST_JSON when set.
 * $UB_TEST_ONLY (a substring) runs only matching tests. Screenshots of failures go to
 * target/test-results/<suite>/.
 */
export async function run(suite, tests) {
  const only = process.env.UB_TEST_ONLY;
  const have = { chrome: hasChrome(), firefox: hasFirefox(), gamedata: gameFiles() !== null };
  const results = [];
  for (const t of tests) {
    if (only && !t.name.includes(only)) continue;
    const missing = (t.needs || []).filter((n) => !have[n]);
    if (missing.length) {
      console.log(`skip  ${suite}: ${t.name} (no ${missing.join(', no ')})`);
      results.push({ suite, name: t.name, status: 'skip', reason: `no ${missing.join(', ')}` });
      continue;
    }
    const start = Date.now();
    const ctx = { shots: join(repo, 'target/test-results', suite), cleanup: [] };
    let status = 'pass', reason;
    try {
      await withTimeout(`the test (${t.timeout ?? TEST_TIMEOUT} s)`, (t.timeout ?? TEST_TIMEOUT) * 1000, t.run(ctx));
    } catch (e) {
      status = e instanceof Skip ? 'skip' : 'fail';
      reason = e.message;
      for (const b of status === 'fail' ? ctx.browsers || [] : []) {
        try { await b.screenshot(join(ctx.shots, `${t.name.replace(/\W+/g, '-')}.png`)); } catch {}
        const errors = b.errors?.() ?? [];
        if (errors.length) reason += `\n      page errors: ${errors.slice(0, 3).join(' | ').slice(0, 600)}`;
      }
    } finally {
      for (const f of ctx.cleanup.reverse()) { try { await f(); } catch {} }
    }
    const secs = ((Date.now() - start) / 1000).toFixed(1);
    const mark = { pass: 'ok  ', fail: 'FAIL', skip: 'skip' }[status];
    console.log(`${mark}  ${suite}: ${t.name} (${secs} s)${reason ? `\n      ${reason}` : ''}`);
    results.push({ suite, name: t.name, status, seconds: Number(secs), ...(reason ? { reason } : {}) });
  }
  if (process.env.UB_TEST_JSON) writeFileSync(process.env.UB_TEST_JSON, JSON.stringify(results, null, 2));
  process.exitCode = results.some((r) => r.status === 'fail') ? 1 : 0;
  return results;
}

/** Launch a browser for a test; it is closed (and screenshotted on failure) automatically. */
export async function launch(ctx, kind, options) {
  const b = kind === 'firefox' ? await firefox(options) : await chrome(options);
  (ctx.browsers ||= []).push(b);
  ctx.cleanup.push(() => b.close());
  return b;
}

/** Serve the built game (UB_WEB_DIST or target/test-web/game) for a test. */
export async function serveGame(ctx, pages) {
  const dist = process.env.UB_WEB_DIST || join(repo, 'target/test-web/game');
  check(existsSync(join(dist, 'index.html')), `no web build in ${dist}: run tools/test/run.py browser (it builds one)`);
  const s = await serve(dist, pages);
  ctx.cleanup.push(() => s.close());
  return s.url;
}

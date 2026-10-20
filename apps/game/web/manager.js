// The manager (#17): the game files, mods and saves this browser keeps, in the game's own
// IndexedDB database and settings. The game reads them once when the page opens, so every
// change reloads the page, with the manager open on the same tab.
(() => {
  const DB = 'banished-wasm', VERSION = 3, SETTINGS = 'banished-wasm.settings';
  const STORES = ['files', 'meta', 'mods', 'game'];
  const $ = (id) => document.getElementById(id);
  const size = (n) => n >= 1e6 ? (n / 1e6).toFixed(1) + ' MB' : Math.max(1, Math.round(n / 1e3)) + ' KB';
  const open = () => new Promise((resolve, reject) => {
    const r = indexedDB.open(DB, VERSION);
    r.onupgradeneeded = () => STORES.forEach((s) => r.result.objectStoreNames.contains(s) || r.result.createObjectStore(s));
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
  const req = (r) => new Promise((resolve, reject) => { r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error); });
  async function all(store) {
    const db = await open();
    try {
      const os = db.transaction(store).objectStore(store);
      const [keys, values] = await Promise.all([req(os.getAllKeys()), req(os.getAll())]);
      return keys.map((k, i) => [String(k), values[i]]);
    } finally { db.close(); }
  }
  async function change(store, f) {
    const db = await open();
    try {
      const tx = db.transaction(store, 'readwrite');
      f(tx.objectStore(store));
      await new Promise((resolve, reject) => { tx.oncomplete = resolve; tx.onerror = () => reject(tx.error); });
    } finally { db.close(); }
  }
  const settings = () => Object.fromEntries((localStorage.getItem(SETTINGS) || '').split('\n')
    .filter((l) => l.includes('=')).map((l) => [l.slice(0, l.indexOf('=')).trim(), l.slice(l.indexOf('=') + 1).trim()]));
  const saveSettings = (s) => localStorage.setItem(SETTINGS, Object.keys(s).sort().map((k) => `${k}=${s[k]}`).join('\n') + '\n');
  const reload = (tab) => { sessionStorage.setItem('ub-manage', tab); location.reload(); };
  const row = (list, name, meta, ...controls) => {
    const li = document.createElement('li');
    const n = document.createElement('span'); n.className = 'name'; n.textContent = name;
    const m = document.createElement('span'); m.className = 'meta'; m.textContent = meta;
    li.append(...controls.filter((c) => c.lead), n, m, ...controls.filter((c) => !c.lead));
    list.append(li);
  };
  const button = (text, onClick) => { const b = document.createElement('button'); b.type = 'button'; b.textContent = text; b.onclick = onClick; return b; };
  const empty = (list, text) => { const li = document.createElement('li'); li.className = 'empty'; li.textContent = text; list.append(li); };

  async function showGame() {
    const list = $('m-game'); list.replaceChildren();
    const files = await all('game');
    files.forEach(([name, blob]) => row(list, name, size(blob.size || 0)));
    if (!files.length) empty(list, 'No game files are kept. Choose them below; they are kept for next time.');
    $('m-forget-game').hidden = !files.length;
  }
  async function showMods() {
    const list = $('m-mods'); list.replaceChildren();
    const s = settings();
    const mods = (await all('mods')).map(([name, bytes]) => ({ name, bytes,
      on: s['ModEnabled_' + name] === 'true', index: Number(s['ModIndex_' + name] ?? 1e9) }))
      .sort((a, b) => a.index - b.index || a.name.localeCompare(b.name));
    const write = () => { const t = settings(); mods.forEach((m, i) => { t['ModEnabled_' + m.name] = String(m.on); t['ModIndex_' + m.name] = String(i); }); saveSettings(t); reload('mods'); };
    mods.forEach((m, i) => {
      const on = document.createElement('input'); on.type = 'checkbox'; on.checked = m.on; on.lead = true;
      on.title = 'Use this mod'; on.onchange = () => { m.on = on.checked; write(); };
      const up = button('↑', () => { [mods[i - 1], mods[i]] = [mods[i], mods[i - 1]]; write(); }); up.disabled = i === 0; up.title = 'Load earlier';
      const down = button('↓', () => { [mods[i + 1], mods[i]] = [mods[i], mods[i + 1]]; write(); }); down.disabled = i === mods.length - 1; down.title = 'Load later';
      const remove = button('Remove', async () => { await change('mods', (os) => os.delete(m.name)); const t = settings(); delete t['ModEnabled_' + m.name]; delete t['ModIndex_' + m.name]; saveSettings(t); reload('mods'); });
      row(list, m.name, size(m.bytes.byteLength || m.bytes.length || 0), on, up, down, remove);
    });
    if (!mods.length) empty(list, 'No mods are kept. Add .pkm files here, or drop them on the page.');
  }
  async function showSaves() {
    const list = $('m-saves'); list.replaceChildren();
    const files = new Map(await all('files'));
    const meta = await all('meta');
    const saves = meta.map(([name, m]) => ({ name, when: new DataView(m.buffer, m.byteOffset, 8).getFloat64(0, true) }))
      .sort((a, b) => b.when - a.when);
    saves.forEach(({ name, when }) => {
      const data = files.get(name);
      const download = document.createElement('a'); download.textContent = 'Download';
      download.href = '#'; download.onclick = (e) => { e.preventDefault();
        const url = URL.createObjectURL(new Blob([data], { type: 'application/octet-stream' }));
        const a = document.createElement('a'); a.href = url; a.download = name + '.sav'; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000); };
      const remove = button('Delete', async () => {
        if (!confirm(`Delete the save "${name}" from this browser?`)) return;
        await change('files', (os) => os.delete(name)); await change('meta', (os) => os.delete(name)); reload('saves'); });
      row(list, name, `${new Date(when).toLocaleString()} · ${size(data ? data.byteLength : 0)}`, download, remove);
    });
    if (!saves.length) empty(list, 'No saves yet. Saves you make in the game appear here; you can also import .sav files.');
  }

  function select(tab) {
    document.querySelectorAll('#manage .tabs button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.tab === tab)));
    document.querySelectorAll('#manage .pane').forEach((p) => { p.hidden = p.dataset.pane !== tab; });
    ({ game: showGame, mods: showMods, saves: showSaves })[tab]().catch((e) => console.error('manager:', e));
  }
  addEventListener('DOMContentLoaded', () => {
    document.querySelectorAll('#manage .tabs button').forEach((b) => b.onclick = () => select(b.dataset.tab));
    $('manage').addEventListener('toggle', () => $('manage').open && select(document.querySelector('#manage .tabs [aria-selected=true]').dataset.tab));
    $('m-forget-game').onclick = async () => { await change('game', (os) => os.clear()); location.reload(); };
    $('help-toggle').onclick = () => {
      const on = document.body.classList.toggle('show-help');
      $('help-toggle').setAttribute('aria-expanded', String(on));
      $('help-toggle').textContent = on ? 'Hide help' : 'Help';
    };
    // The save import reloads the page from the game's side; reopen on the Saves tab.
    $('import-saves').addEventListener('change', () => sessionStorage.setItem('ub-manage', 'saves'));
    $('add-mods').onchange = async (e) => {
      const files = [...e.target.files].filter((f) => /\.pkm$/i.test(f.name));
      for (const f of files) { const bytes = new Uint8Array(await f.arrayBuffer()); await change('mods', (os) => os.put(bytes, f.name.replace(/\.pkm$/i, ''))); }
      if (files.length) reload('mods');
    };
    const tab = sessionStorage.getItem('ub-manage') || (location.hash === '#manage' ? 'game' : null);
    sessionStorage.removeItem('ub-manage');
    if (tab) { $('manage').open = true; select(tab); }
  });
})();

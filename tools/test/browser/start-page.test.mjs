// The browser version's start page (issue #17): the first visit, kept game files, Help, and the
// manager's game files, mods and saves. Run by tools/test/run.py browser, or alone:
//   node tools/test/browser/start-page.test.mjs
import { check, equal, gameFiles, launch, optionalFile, run, serveGame, Skip } from './lib.mjs';

const shown = (id) => `(() => { const e = document.getElementById(${JSON.stringify(id)}); return !!e && e.offsetHeight > 0; })()`;

// What the page shows: which parts are on screen, and the status line.
const LAYOUT = `({
  kept: ${shown('kept')}, manager: ${shown('manage')}, steps: ${shown('steps')},
  keepBox: document.getElementById('keep').checked,
})`;

// The rows of a manager tab, after opening it.
const rows = (tab) => `(async () => {
  document.querySelector('[data-tab=${tab}]').click();
  await new Promise((r) => setTimeout(r, 300));
  return [...document.querySelectorAll('#m-${tab === 'game' ? 'game' : tab} li')]
    .map((li) => ({ name: li.querySelector('.name')?.textContent ?? li.textContent,
                    on: li.querySelector('input[type=checkbox]')?.checked ?? null }));
})()`;

/** Reload the page and wait until the game's code runs again. */
async function reload(b) {
  await b.expectReload(() => b.eval('void setTimeout(() => location.reload(), 20), true'));
  await b.waitFor(`document.getElementById('status').textContent !== 'Starting…'`);
}

/** Choose the game files (and `extra` files) once, so that they are kept, then reload. */
async function keepFiles(b, url, extra = []) {
  await b.open(`${url}/`);
  await b.chooseFiles([...gameFiles(), ...extra]);
  await b.waitMenu();
  await reload(b);
}

await run('start-page', [
  {
    name: 'A first visit shows the steps only',
    needs: ['chrome'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await b.open(`${url}/`);
      equal(await b.eval(LAYOUT), { kept: false, manager: false, steps: true, keepBox: true }, 'first visit');
    },
  },
  {
    name: 'Kept game files show Start, Help and the manager',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await keepFiles(b, url);
      equal(await b.eval(LAYOUT), { kept: true, manager: true, steps: false, keepBox: true }, 'second visit');
      equal((await b.eval(rows('game'))).map((r) => r.name),
        ['data0.pkg', 'data1.pkg', 'patch0.pkg', 'patch1.pkg'], 'kept game files');
      await b.eval("document.getElementById('help-toggle').click(), true");
      equal(await b.eval(`[${shown('steps')}, document.getElementById('help-toggle').textContent]`),
        [true, 'Hide help'], 'after Help');
      await b.eval("document.getElementById('start-kept').click(), true");
      await b.waitMenu();
    },
  },
  {
    name: 'Forget the game files returns to the first visit',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await keepFiles(b, url);
      await b.eval("document.querySelector('[data-tab=game]').click(), true");
      await b.expectReload(() => b.eval("void setTimeout(() => document.getElementById('m-forget-game').click(), 20), true"));
      await b.waitFor(`!document.body.classList.contains('has-kept') && ${shown('steps')}`);
    },
  },
  {
    name: 'A mod switched on stays on after the page reloads',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const mods = ['apiary.pkm', 'BetterFields.pkm'].map((m) => optionalFile('game-data/modtest/WinData', m));
      if (mods.includes(null)) throw new Skip('no mods in game-data/modtest/WinData (apiary.pkm, BetterFields.pkm)');
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await keepFiles(b, url, mods);
      equal(await b.eval(rows('mods')), [{ name: 'apiary', on: false }, { name: 'BetterFields', on: false }], 'kept mods');
      await b.expectReload(() => b.eval(`void setTimeout(() => [...document.querySelectorAll('#m-mods li')]
        .find((li) => li.textContent.includes('BetterFields')).querySelector('input').click(), 20), true`));
      await b.waitFor(`document.body.classList.contains('has-kept')`);
      equal(await b.eval(rows('mods')), [{ name: 'apiary', on: false }, { name: 'BetterFields', on: true }], 'after the switch');
      const settings = await b.eval("localStorage.getItem('banished-wasm.settings')");
      check(settings.includes('ModEnabled_BetterFields=true'), 'the settings say BetterFields is on');
    },
  },
  {
    name: 'An imported save is listed with its size',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const save = optionalFile('save-game/Save0.sav');
      if (!save) throw new Skip('no save-game/Save0.sav');
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await keepFiles(b, url);
      await b.expectReload(() => b.setFiles('#import-saves', [save]));
      await b.waitFor(`document.body.classList.contains('has-kept')`);
      await b.eval("document.querySelector('[data-tab=saves]').click(), true");
      await b.waitFor(`[...document.querySelectorAll('#m-saves li')].some((li) => li.textContent.startsWith('Save0'))`);
      const saves = await b.eval("[...document.querySelectorAll('#m-saves li')].map((li) => li.textContent)");
      equal(saves.length, 1, 'saves listed');
      check(/^Save0.*MB/.test(saves[0]), `the save row names Save0 and its size: ${saves[0]}`);
    },
  },
]);

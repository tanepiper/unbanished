// A town in the browser version: the same new game as the native probe baseline
// (tools/test/baselines/new-game.json), and the console's `unbanished` object. Run by
// tools/test/run.py browser, or alone: node tools/test/browser/game.test.mjs
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { check, equal, gameFiles, launch, repo, run, serveGame, sleep } from './lib.mjs';

// The native run's answers at 16 s, a few seconds after its new game started.
const baseline = JSON.parse(readFileSync(join(repo, 'tools/test/baselines/new-game.json'), 'utf8'));
const native = (command) => baseline.find((a) => a.command === command && a.t === 16).ok;

await run('game', [
  {
    name: 'A new game in the browser starts with the native town',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      await b.newGame(1234, 'Testwood');
      // Pause at once, so the town stays as it started while it is compared.
      await b.probe('script toggle_pause');
      equal(await b.probe('citizens'), native('citizens'), 'citizens');
      equal(await b.probe('buildings'), native('buildings'), 'buildings');
      const town = await b.probe('town');
      equal({ name: town.name, population: town.population, food: town.food, firewood: town.firewood },
        { name: 'Testwood', population: native('town').population, food: native('town').food, firewood: native('town').firewood },
        'town');
    },
  },
  {
    name: 'A fresh browser shows the full page as its Resolution',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      await b.press('ref:MainMenu text:Options');
      await b.waitDialog('Options');
      await b.press('ref:Options text:Video');
      const [combo] = await b.probe('ui ref:Options comboResolution');
      // The page is 1280x720 (lib.mjs VIEW); 100 % is the scene drawn without a setting.
      equal(combo?.text, '1280x720', 'Resolution with no setting');
    },
  },
  {
    name: 'Firefox with WebGPU offered but no WebGPU canvas falls back to WebGL 2',
    needs: ['firefox', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      // Headless Firefox offers navigator.gpu but gives the canvas no WebGPU context.
      const b = await launch(ctx, 'firefox', { webgpu: true });
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      check(b.logs.some((l) => l.includes('using WebGL 2')), 'the game said it fell back to WebGL 2');
    },
  },
  {
    name: 'The console object answers every command',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      const names = await b.eval("unbanished.ui('ref:MainMenu kind:button').then((l) => l.map((e) => e.text))");
      equal(names, ['New', 'Load', 'Multiplayer', 'Tutorials', 'Achievements', 'Options', 'Mods', 'Exit'], 'main menu buttons');
      const error = await b.eval("unbanished.town().then(() => 'no error', (e) => e.message)");
      equal(error, 'town: no town in Game/Menu', 'town in the menu');
      check(await b.eval("typeof unbanished.help === 'function' && typeof unbanished.citizens === 'function'"),
        'help() and the town methods exist');
      await b.eval("unbanished.click('text:New')");
      await b.waitDialog('NewGame');
      const combos = await b.eval("unbanished.ui('ref:NewGame kind:combo').then((l) => l.length)");
      check(combos >= 5, `the New Game dialog has its combos (${combos})`);
    },
  },
  {
    name: 'After a new game the camera moves, turns and zooms with the keys, the wheel and the middle button',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      await b.newGame(1234, 'Testwood');
      const camera = () => b.probe('camera');
      // Past the fade from black: wait until the camera has stopped easing.
      let last = null;
      for (let i = 0; i < 150; i++) {
        const c = await camera();
        if (last && JSON.stringify(c) === JSON.stringify(last)) break;
        last = c;
        await sleep(200);
      }
      const [w, h] = await b.eval('[window.innerWidth, window.innerHeight]');
      const [cx, cy] = [Math.round(w / 2), Math.round(h / 2)];
      const near = (a, c) => Math.abs(a - c) < 1e-3;

      let before = await camera();
      await b.holdKey('w', 800);
      let after = await camera();
      check(!(near(after.x, before.x) && near(after.y, before.y)),
        `W moves the camera (${before.x},${before.y} → ${after.x},${after.y})`);
      check(near(after.yaw, before.yaw), 'W does not turn the camera');

      before = after;
      await b.holdKey('e', 800);
      after = await camera();
      check(!near(after.yaw, before.yaw), `E turns the camera (yaw ${before.yaw} → ${after.yaw})`);

      before = after;
      await b.wheel(cx, cy, -240);
      after = await camera();
      check(!near(after.zoom, before.zoom), `the wheel zooms (target ${before.zoom} → ${after.zoom})`);

      before = after;
      await b.drag('middle', cx, cy, cx + 200, cy);
      after = await camera();
      check(!near(after.yaw, before.yaw), `a middle drag turns the camera (yaw ${before.yaw} → ${after.yaw})`);
    },
  },
]);

// A town in the browser version: the same new game as the native probe baseline
// (tools/test/baselines/new-game.json), and the console's `unbanished` object. Run by
// tools/test/run.py browser, or alone: node tools/test/browser/game.test.mjs
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { check, equal, gameFiles, launch, repo, run, serveGame } from './lib.mjs';

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
]);

// Exit and Quit in the browser version (issue #18): what each one does in a tab, in a frame
// (the website's Play page) and in an installed app. Run by tools/test/run.py browser, or alone:
//   node tools/test/browser/exit.test.mjs
import { check, equal, gameFiles, launch, run, serveGame, sleep } from './lib.mjs';

// The start page is back: the panel shows, and kept game files offer Start.
const START_PAGE = `!document.getElementById('panel').hidden
  && document.body.classList.contains('has-kept')
  && document.getElementById('status').textContent.includes('Ready')`;

// A page that holds the game in a frame and listens for its exit message, like PlayGame.vue.
const FRAME_PAGE = `<!doctype html><title>frame</title><body style="margin:0">
<iframe id="game" src="/" style="width:1280px;height:720px;border:0"></iframe>
<script>addEventListener('message', (e) => {
  if (e.origin === location.origin && e.data === 'un-banished-exit') {
    document.getElementById('game').remove(); document.title = 'exited';
  }
});</script></body>`;

await run('exit', [
  {
    name: 'Exit on the main menu returns a tab to the start page',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      await b.pressAndLeave('ref:MainMenu text:Exit');
      await b.waitFor(START_PAGE);
    },
  },
  {
    name: 'Exit in a running game returns a tab to the start page',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      await b.newGame();
      await b.key('Escape');
      await b.waitDialog('Pause.rsc');
      await b.pressAndLeave('ref:Pause.rsc text:Exit');
      await b.waitFor(START_PAGE);
    },
  },
  {
    name: 'Quit in a running game returns to the main menu',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      await b.newGame();
      await b.key('Escape');
      await b.waitDialog('Pause.rsc');
      await b.press('ref:Pause.rsc text:Quit');
      await b.waitLoop('Game/Menu');
      check(await b.eval("document.getElementById('panel').hidden"), 'the start page came back');
    },
  },
  {
    name: 'Exit in a frame asks the page around it to close the game',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx, { '/frame.html': FRAME_PAGE });
      const b = await launch(ctx, 'chrome');
      // Keep the game files first, so the framed page can start without the file picker.
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      await b.eval(`void setTimeout(() => { location.href = '/frame.html'; }, 50), true`);
      const frame = "document.getElementById('game').contentWindow";
      await b.waitFor(`${frame}.document.body.classList.contains('has-kept')
        && ${frame}.document.getElementById('status').textContent.includes('Ready')`);
      await b.eval(`${frame}.document.getElementById('start-kept').click()`);
      await b.waitFor(`${frame}.unbanished.probe('state').then((s) => s.loop === 'Game/Menu').catch(() => false)`, 120000);
      await b.pressAndLeave('ref:MainMenu text:Exit', frame);
      await b.waitFor(`document.title === 'exited'`);
    },
  },
  {
    name: 'Exit on the main menu closes an installed app',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome', { standalone: true });
      await b.open(`${url}/`);
      equal(await b.eval('history.length'), 1, 'history entries of a new app window');
      await b.startWithFiles(gameFiles());
      await b.pressAndLeave('ref:MainMenu text:Exit');
      await b.waitClosed();
    },
  },
  {
    name: 'Exit in a running game closes an installed app',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome', { standalone: true });
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      await b.newGame();
      await b.key('Escape');
      await b.waitDialog('Pause.rsc');
      await b.pressAndLeave('ref:Pause.rsc text:Exit');
      await b.waitClosed();
    },
  },
  {
    name: 'Firefox: Exit on the main menu returns to the start page',
    needs: ['firefox', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'firefox');
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());
      await b.pressAndLeave('ref:MainMenu text:Exit');
      await b.waitFor(START_PAGE);
    },
  },
]);

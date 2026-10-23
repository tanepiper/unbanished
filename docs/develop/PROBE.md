# The probe: look at and drive a running game

The probe is a small command language for a running Un-Banished. It finds things on the screen
(buttons, edits, dialogs) and in the town (buildings, citizens, animals, stock), and it presses
buttons and types text as a player would. It is not part of the original game: it exists for
tests, scripted recordings and curious players. Every answer is JSON.

You can use it in three places, with the same commands:

| Where | How |
|---|---|
| The browser's console | `await unbanished.town()`, `await unbanished.click('text:New')` |
| A headless run | `unbanished --boot-capture last.png 20 --probe "town@16" --probe-out answers.jsonl` |
| The test suites | `tools/test/scenarios.json` and the browser tests in `tools/test/browser/` ([Testing](TESTING.md)) |

The probe only reads the game, except for `click`, `type`, `combo` and `script`. These go
through the same paths as a player's mouse, keyboard and capture script, so a game driven by the
probe is the same game a player gets with the same input. The code is in
`apps/game/src/probe.rs` (the commands), `apps/game/src/gameplay/probe.rs` (the town) and
`crates/ui/src/query.rs` (finding elements).

## In the browser's console

Start the game in a browser (the [Play page](/play) or `tools/dev-web.sh --app game --release`),
open the browser's developer tools (F12, or Cmd+Option+I on a Mac) and choose **Console**. The
game page has a global object, `unbanished`. Each method returns a Promise, so put `await` in
front of it. On the Play page the game runs in a frame: in the console's context menu (it says
**top** at first), choose the frame whose address ends in `game/index.html`.

```js
unbanished.help()                      // lists every method

await unbanished.state()
// { loop: "Game/Menu", dialogs: ["Dialog\\BuildId.rsc:main", "Dialog\\MainMenu.rsc:main"], quit: false }

await unbanished.ui('ref:MainMenu kind:button')
// [ { name: "buttonNew", text: "New", kind: "button", enabled: true, rect: [560, 259, 720, 289], … },
//   { name: "buttonLoad", text: "Load", … }, …, { name: "buttonQuit", text: "Exit", … } ]

await unbanished.click('text:New')      // opens the New Game dialog
await unbanished.type('editName', 'Testwood')
await unbanished.type('editSeed', '1234')
await unbanished.combo('comboClimate', 'Harsh')
await unbanished.click('ref:NewGame buttonOk')
```

Once a town is running:

```js
await unbanished.town()
// { name: "Testwood", year: 1, month: 0, population: 19, adults: 10, students: 0, children: 9,
//   health: 10, happiness: 10, food: 1800, firewood: 200, buildings: 2, animals: 110, paused: false }

await unbanished.citizens()
// { adults: 10, students: 0, children: 9, by_profession: { laborer: 10 } }

await unbanished.buildings('storage')
// { built: { total: 2, by_name: { StorageBarn: 1, StorageYard: 1 } },
//   under_construction: { total: 0, by_name: {} } }

await unbanished.animals('deer')        // { total: 110, by_name: { WildAnimalDeer: 110 } }
await unbanished.stock()                // { wood: 120, stone: 60, iron: 40, firewood: 200, tools: 35, food: 1800, … }

await unbanished.script('speed 10')     // the fastest game speed
await unbanished.script('pause')        // the pause menu
await unbanished.click('ref:Pause.rsc text:Quit')
```

Two small recipes:

```js
// Log the town's food and population every 10 seconds.
setInterval(async () => {
  const t = await unbanished.town();
  console.log(`year ${t.year} month ${t.month}: ${t.population} people, ${t.food} food`);
}, 10000);

// Which buttons are on screen now, and what are they called in the game's resources?
console.table((await unbanished.ui('kind:button')).map((b) => ({ name: b.name, text: b.text, enabled: b.enabled })));
```

`unbanished.probe(text)` runs any command as text, for example
`unbanished.probe('ui ref:Options kind:combo')`. The game answers between frames, so it must be
running; before you press **Start the game**, every call fails with "the game is not loaded".

## In a headless run

`--probe "COMMAND@SECONDS"` runs a command at a time in a headless capture. Each answer is
logged, and with `--probe-out FILE` also written to FILE as one JSON object per line:
`{"t": 16.0, "command": "town", "ok": {…}}`, or `"error"` in place of `"ok"`.

```bash
UNBANISHED_USER_DIR=/tmp/ub-user UNBANISHED_SETTINGS=/tmp/ub-settings.txt UNBANISHED_SEED=1234 \
target/release/unbanished --boot-capture /tmp/last.png 42 --capture-size 1280x720 \
  --probe "click ref:MainMenu text:New@4.5" \
  --probe "type editName=Testwood@4.8" --probe "type editSeed=1234@4.9" \
  --probe "click ref:NewGame buttonOk@5" \
  --probe "town@16" --probe "script speed 10@16.5" --probe "town@40" \
  --probe-out /tmp/answers.jsonl
```

The capture runs as fast as the computer can draw it, not in real time: this example takes
about two seconds. Times are seconds of capture time, counted from the start; the main menu is
ready at about 4.5 s. A new game is running about 10 s after its Ok.

`UNBANISHED_USER_DIR` and `UNBANISHED_SETTINGS` keep the run away from your own saves and settings.
`UNBANISHED_SEED` fixes the game's random numbers, so two runs give the same answers.

## Finding elements: queries

`ui`, `click` and the browser's `ui()` and `click()` take a query. A query is a list of steps
separated by spaces. Each step looks inside what the step before it found.

| Step | Finds | Example |
|---|---|---|
| `NAME` | the element with this name in its resource | `buttonNew`, `editSeed` |
| `text:TEXT` | the element whose text is TEXT (letter case ignored); quote text with spaces | `text:Exit`, `text:"Wood House"` |
| `kind:KIND` | elements of this kind | `kind:button`, `kind:combo` |
| `ref:PART` | elements whose resource reference contains PART: usually a dialog | `ref:MainMenu`, `ref:Pause.rsc` |
| `*` | any element | `ref:Options *` |

The kinds are `button`, `text`, `label`, `image`, `group`, `ribbon`, `border`, `list`,
`viewport`, and for controls `check`, `slider`, `scroll`, `edit`, `combo`, `spinner`, `tabs`,
`table`, `container`, `progress` and a few more. Only shown elements count: a hidden element and
everything inside it are skipped. Dialogs are searched from the top one down. The matches come
in tree order. A button's text is the text of the label inside it, so `text:Exit` finds
both the button and its label; the button comes first. `click` takes the first usable match, so
`click text:Exit` presses the button.

Prefer text over names when you write a test: the names in the original's resources are not
always what you would guess. The main menu's **Exit** is `buttonQuit`; in the pause menu
**Quit** is `buttonMenu` and **Exit** is `buttonQuit`. `ui kind:button` lists the names.

Each element in an answer has `name`, `text` (for a combo, the choice it shows), `kind`, `ref` (its resource), `rect` (left, top,
right, bottom in pixels after the last layout) and `enabled`; check boxes add `checked`, and
sliders, scroll bars and progress bars add `value`.

## Every command

| Command | Answer | Changes the game? |
|---|---|---|
| `state` | the current loop (`DialogLoaderLoop`, `Game/Menu`, `Game/Game`) and the open dialogs, top first | no |
| `ui QUERY` | every shown element matching QUERY | no |
| `click QUERY` | clicks the first usable match at its centre, as a mouse does; answers with that element | as a click |
| `type NAME=TEXT` | clicks the edit NAME, empties it and types TEXT | as typing |
| `combo NAME=ITEM` | selects the item ITEM (its text) in the combo NAME | as a choice |
| `script COMMAND` | a capture script command: `speed N` (1, 2, 5, 10), `pause`, `toggle_pause`, `place TEMPLATE`, `open TEMPLATE`, `save PATH` (headless only) | as the command |
| `town` | the town's date, population, health, happiness, food, firewood, buildings and animals | no |
| `stock` | the Status Bar's stock: wood, stone, iron, firewood, tools, food, herbs, clothes, alcohol, coal, textile | no |
| `buildings [FILTER]` | buildings counted by template, built and under construction | no |
| `citizens [FILTER]` | adults, students, children, and the working adults by profession | no |
| `animals [FILTER]` | animals counted by template | no |
| `key LETTER down\|up` | a letter key pressed or released, as the window handler sends it (so held keys, such as W or E for the camera, act between the two) | as the key |
| `wheel STEPS` | mouse-wheel steps (positive zooms in) | as the wheel |
| `drag DX DY` | the mouse's movement while the rotate button (the middle button) is held | as the drag |
| `camera` | the game camera: its pivot `x`, `y` on the ground, `yaw`, `pitch`, `distance`, and the `zoom` distance it eases towards | no |

FILTER keeps the names that contain it (letter case ignored): `buildings house`,
`animals deer`. Counts are sorted by name, so the same town always gives the same JSON. In the
menus, the town commands answer with an error ("no town in Game/Menu").

## How it fits together

- `crates/ui/src/query.rs` parses queries and searches the element tree (`Ui::query`,
  `Ui::info`). It has unit tests and needs no game data.
- `apps/game/src/probe.rs` runs a command against the game: it lays out the screen, finds
  elements, clicks through `Game::press_node` (the pointer path the player's mouse takes), and
  asks the current loop for town answers (`GameLoop::probe_world`).
- `apps/game/src/gameplay/probe.rs` answers the town commands from the town's own state and the
  Town Hall's numbers (`status_values`).
- In the browser, `window.unbanished` (in `apps/game/index.html`) queues a command with the
  game's exported `unbanishedProbe` function; the game answers between frames and the page
  collects the answer with `unbanishedProbeResult`.
- In a headless run, `--probe` commands run at their time in the capture loop
  (`apps/game/src/main.rs`).

To add a command, add it to `probe::run` (UI and game) or to `GameplayLoop::world_query` (town),
keep its answer sorted and free of times or addresses, describe it in the table above, and add it
to a scenario in `tools/test/scenarios.json`.

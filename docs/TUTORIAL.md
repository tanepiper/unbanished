# Tutorials and scenarios

The main menu's Tutorial (12) and Scenario (9) buttons list `Game\Scenario\ScenarioList.rsc`
(an ExternalList of `Scenario`, `FUN_1400ae840`). A Scenario is an `Achievement::Achievement`
(string table, title, description, sprite, goals) plus the game it starts: `+0x60` the seed,
`+0x64`, `+0x58` the town's name, `+0x70` the start condition, `+0x78` the terrain, `+0x80` the
climate, `+0x68`, `+0x69` two flags, and `+0x88` a save to start from (`FUN_1400144b0`). With a
save name the game loads `Save\<name>.sav` from the packages (`FUN_1400e6240`: `SaveLoad`
operation 3); without one it starts a new game from the fields (`FUN_14009d530`). The four
shipped scenarios are the tutorials, each with a packaged save (`Save\TutorialGettingStarted.sav`,
`TutorialFood`, `TutorialSurvival`, `TutorialTrade`).

## The tutorial

A scenario's goal is a `FinishTutorialGoal` naming a `Tutorial`. Its instance's startup
(`FUN_1400dcf60`) makes a `TutorialInstance` at step 0, opens the tutorial's window
(`Tutorial +0x20`, a UIDescription; `FUN_1401056c0` at the screen's right edge less 464) and
sets game flags `0x5f0` (`Game +0xd0`). The goal is met when every step is done
(`FUN_1400dd080`). Persisted, the instance resumes at the latest checkpoint step
(`TutorialStep +0x20`, `FUN_1400e69f0`).

`Tutorial` (`FUN_1400e3fa0`): `+0x28` the steps, `+0x20` the window, `+0x38` the scenario to
start when it is done (none below 0).

`TutorialStep` (`FUN_1400e8770`): `+0x20` a checkpoint, `+0x28` the string table, `+0x30` and
`+0x38` the texts before and after, `+0x40` images, `+0x50` setups, `+0x60` goals.

### The step machine (`FUN_1400e7280`, the goal instance's update)

1. With no window yet, open it (`FUN_140105740`), take its TutorialUI controller and enter the
   step.
2. Update every setup (slot `+0x48`) and test every goal (slot `+0x40`).
3. All goals met: every setup's exit (slot `+0x50`), the next step; entered if there is one,
   else the window closes (`FUN_140106f50`) and the next scenario starts (`FUN_1400e6240`).

Entering a step (`FUN_1400e6730`): every setup's enter (slot `+0x40`); TutorialUI's `preText`
and `postText` take the step's texts (hidden when empty); `imageTable` is emptied and each
image's element (slot `+0x48`) goes in its cell (`+0x28` column, `+0x2c` row).

### TutorialUI (`0x33161e00`, vtable `0x140167350`)

Bind `FUN_1400e8c70`: `preText`, `postText`, `imageTable`, `nextButton` (id `0x578`),
`quitButton` (`0x579`); the host's `buttonClose` is hidden. Refresh `FUN_1400e8bd0`: the two
buttons are shown while enabled. Quit (`FUN_1400e8bb0`) returns to the main menu.

### Setups (`TutorialSetup`; slots `+0x40` enter, `+0x48` update, `+0x50` exit)

- `TutorialSetupDialog` (`FUN_1400e7f30`): `+0x20` the elements left usable, `+0x30` more,
  `+0x40` the element to point at with `+0x50` (`Dialog\TutorialHighlight.rsc`) and `+0x48`
  (an angle), `+0x58` a ComponentDescription and `+0x70`. Enter `FUN_1400e7ac0`, update
  `FUN_1400e7990`, exit `FUN_1400e7930`.
- `TutorialSetupCamera` (`FUN_1400e7680`): two cells (`+0x20`/`+0x24`, `+0x28`/`+0x2c`); enter
  (`FUN_1400e75a0`) has the camera go to them (game UI `+0xa4`, `+0xa8`, `+0xac`).
- `TutorialSetupGameSpeed` (`FUN_1400e82c0`): enter (`FUN_1400e8230`) sets the speed (`+0x34`)
  and, with `+0x24`, unpauses (`+0x30`).
- `TutorialSetupRestrictPlayer` (`FUN_1400e8680`): a rectangle of cells (`+0x20`..`+0x2c`),
  `+0x30`, and two templates (`+0x38`, `+0x50`); enter (`FUN_1400e83f0`) restricts what the
  player may place to them there (game UI `+0x80`..`+0xa0`), the camera going there with a
  first template; exit (`FUN_1400e8360`) lifts it.

### Goals (`TutorialGoal`, slot `+0x40`)

- `TutorialGoalButton` (`FUN_1400e41b0`): the element at the path `+0x20` was pressed.
- `TutorialGoalCount` (`FUN_1400e4760`): a count (`+0x20` kind) against `+0x24`, at least it
  (`+0x28`) or exactly: 0 entities of the template `+0x30`, 1 working adults of the profession,
  2 citizens, 3 sites of the template, 4 stock manager `+8`, 5 map `+0xf0` above 0, 6 workers at
  the template's workplaces, 7 what the trading posts keep of the material, 8 what they hold, 9
  the value of the spinner at `+0x48`, 10 the template's built entities.
- `TutorialGoalDialog` (`FUN_1400e43b0`): the dialog `+0x20` is open (on the entity of
  `+0x28`).
- `TutorialGoalGameSpeed` (`FUN_1400e4620`): the game speed is `+0x20`.

### Images (`TutorialImage`, slot `+0x48` makes the element)

`+0x28` column, `+0x2c` row, `+0x20` the element. `TutorialImageHotKey` labels it with a
toolbar item's key (`+0x30`), `TutorialImageInputKey` with a key-map action's (`+0x30`),
`TutorialImageToolbarIcon` shows a toolbar item's icon.

Formats: `crates/formats/src/scenario.rs` (every shipped part parses whole).

## Paths

The setups and goals name elements by path (`FUN_1400e6d40`, split at `:` by `FUN_1400e6160`):
`ToolbarDialog:<item>` is the toolbar's button for `Game\Toolbar.rsc:<item>`;
`DialogInstance:<controller>:<element>[:<template>]` the named element in an open window with a
controller of that class (an entity's only when its template is `<template>`).

Locking (`FUN_1400e7720`) sets element flag `0x1000000` on an element and everything in it;
input passes a locked element by (the port: `flags::LOCKED`, `Node::usable`) but keeps its
children, so an element unlocked inside a locked dialog works.

## The highlight (`TutorialHighlightDialog`, `Dialog/TutorialHighlight.rsc`)

Bind `FUN_1400e54f0` finds `glowGroup`, `glow` and `arrow`. Each frame (`FUN_1400e54b0`) the glow
is hidden, and the arrow is placed and turned by `-angle` about its centre (`SpriteContext::Flush`:
`x·cos − y·sin`, `x·sin + y·cos`, `Math::Scalar::Sin` is `FUN_18012a620`). It bobs by
`sin(t·π)·8` pixels. Placement depends on the target:
- **An element** (`+0x40`, `FUN_1400e4f90`): the arrow's box is a square of the sprite's width,
  set against a corner of the element's rectangle. The sprite points down and right at 0°, so
  its tip lands on that corner. It bobs along the same diagonal:

  | Angle | Box | Corner |
  |---|---|---|
  | 0 (and any other) | `(left − w, top − w)` | top left |
  | 90 | `(left − w, bottom)` | bottom left |
  | 180 | `(right, bottom)` | bottom right |
  | 270 | `(right, top − w)` | top right |

- **An entity** (the `+0x70`th entity of the template `+0x58`, projected by `FUN_140105850`;
  `FUN_1400e5270`): the arrow is centred over the point with its foot there, and bobs up and down.
The setup's dialog opens a frame after the tutorial's window, so it draws above it.

## RestrictPlayer in the game

- The tool manager (`Game +0x120`, `FUN_140107b70`) keeps the player's tool on the template
  `+0x88`; a toolbar build button starts it instead (`FUN_140108030`); cancelling is ignored.
- The PlacementTool (`FUN_140102b20` placing, `FUN_1401033b0` hovering) is valid only on the
  rectangle `+0x98` (corner cells as `y·n + x`) to `+0x9c`, turned `+0xa0` degrees.
- Selection (`FUN_140102500`) takes only entities of the template `+0x90` (none without one).
- The drag tools (removal `FUN_140108ab0`, area `FUN_140100fb0`/`FUN_140100dd0`, fenced
  `FUN_1400e0960`/`FUN_1400e02d0`, road `FUN_14010a4b0`/`FUN_14010a200`; not the bridge) take
  only a drag over exactly that rectangle (a road between its corners): the CreateBar shows
  message 3 and the apply fails otherwise (`RemovalTool::restrict`).

## The menus

The Tutorial and Scenario buttons open a `ScenarioDialog` (`FUN_14007db80` with `+0xb8` 1 for
the tutorials, `Dialog\Tutorial.rsc`, and 0 for the others, `Dialog\Scenario.rsc`). It lists the
scenarios whose `+0x69` matches (`Dialog\Scenario.rsc:groupAchievement` rows: `imageIcon`,
`textTitle`, `textDescription`; flag `0x10000` makes a row take the mouse); a pressed row is
chosen (`selectSave` shown), OK (`0x65`, enabled once one is chosen) starts it
(`FUN_14007d180`), Cancel (100) closes it.

The pause menu (`MainMenuDialog`, `FUN_1400b91a0`) shows Goal (10) with a scenario (`Game +0x15c`
≥ 0) and Map (11) without one; game flag `0x400` (a tutorial's `0x5f0`) disables Save and Mod.
Map opens `MapParamsDialog` (`Dialog\MapParams.rsc`, `FUN_1400b85f0`; `map_params.rs`): the
town's name (editable; New Name draws one on the game's Random, OK renames the town), seed,
terrain, size, climate, disasters and start condition.
`GoalDialog` (`Dialog\Goal.rsc`, `FUN_14006a170` / `FUN_140069fa0`) shows the goal's title and
a `groupGoal` per goal (`textGoal`, `imageComplete` when met, `imageFail` when failed).

When the game's goal is met or failed (`FUN_14009bd00`) it logs event `0x1a` or `0x1b` with its
title, once.

Port: `apps/game/src/gameplay/tutorial.rs` (the instance, setups, goals, images, highlight,
scenario start), `scenario_ui.rs` (the dialog), `achievement_ui.rs` (`GoalDialog`).

Deviations: the tutorials start a new game from the scenario's fields instead of loading their
packaged saves until saved games load (a resumed tutorial's checkpoint is read then); a text
whose key its table lacks shows nothing (the port tells a missing key by `Resources::string`
returning it).

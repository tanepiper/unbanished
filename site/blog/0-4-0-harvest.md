---
title: 'Harvest: Un-Banished 0.4.0'
name: Harvest
version: 0.4.0
date: 2026-10-21
cover: /assets/videos/autumn-orbit-poster.webp
summary: Mods can now change how a town plays, a multiplayer server can keep and run your town, and Pure Banished plays the original with nothing added. The browser version now keeps your game files.
description: Un-Banished 0.4.0 adds rule hooks for mods, a multiplayer server that keeps and runs your town, Pure Banished, and a browser version that keeps your game files.
outline: [2, 2]
---

<ReleaseHero
  name="Harvest"
  version="0.4.0"
  date="2026-10-21"
  video="/assets/videos/autumn-orbit.webm"
  poster="/assets/videos/autumn-orbit-poster.webp"
  alt="The camera circles a market square in autumn, with yellow trees beside the stalls and fields beyond"
/>

<p class="ub-lead">
Un-Banished 0.4.0 adds rule hooks, which let a mod change how a town plays. It also adds a
multiplayer server that can keep and run your town, a Pure Banished mode that plays only the
original game, and a browser version that keeps your game files.
</p>

## Rule hooks

Banished mods can add buildings, crops and other data, but they cannot change how the town
behaves. In 0.4.0 a mod can include a rule hook: a small WebAssembly program that the game runs
after every update of the town. A rule hook can read the date, the number of citizens and the
stock, use the game's random numbers, and add lines to the event log.

<Shot
  src="/assets/screenshots/rule-hooks-festival.webp"
  alt="The Event Log over a town in autumn: Autumn 22, the town holds its harvest festival, and in orange, a bountiful harvest"
  caption="The example mod: a harvest festival on the first day of Autumn. One year in four it is a bountiful one."
/>

To use a rule hook, put its `.wasm` file in the `Hooks` folder next to your `Save` folder. Every
town you start or load then uses it, and saves list it with the other mods.

A rule hook must give the same result on every computer, or multiplayer games and replays would
go out of step. So the game only accepts hooks that use whole numbers, call no functions except
the game's own, and take every random number from the town's own random numbers. A hook that
runs for too long is stopped and switched off.

In multiplayer, every player needs the same hooks. A player whose `Hooks` folder is different
cannot join the room, and is told why.

[Writing a rule hook](/guide/rule-hooks) shows how to build the harvest festival example in Rust.

## Pure Banished

Everything Un-Banished adds to the original game is now a separate extension: multiplayer, the
mod sites, the probe, recordings, the film camera and rule hooks. The town itself is now in its
own part of the code, with no drawing, sound or windows.

Start the game with `--pure` (in the browser, add `?pure` to the address) to turn off every
extension for that game. You get Banished 1.0.7 and its own mods only: no Multiplayer button, no
Browse tabs, and no rule hooks.

<div class="ub-shots">
<Shot
  src="/assets/screenshots/main-menu-unbanished.webp"
  alt="The main menu over a snowy harbour town, with a Multiplayer button under Load"
  caption="The main menu with extensions."
/>
<Shot
  src="/assets/screenshots/main-menu-pure.webp"
  alt="The main menu over a river town in summer, with the original's buttons only"
  caption="The main menu in Pure Banished."
/>
</div>

A test checks that the game builds with no extensions, and that this build plays every test town
to exactly the same result as the full build.

## The multiplayer server

In 0.3.0 the relay only passed messages between players. When the host closed the game, the
town was lost.

The relay is now a server, `unbanished-server`. With it:

- players can join a game that has already started, and a player who left can join again;
- the server checks that every player has the same town, and if one is different, the town that
  most players have is kept;
- if its settings name a folder, the server keeps each room's town, so the host can open the
  room again later and continue;
- if you give it a copy of the game data, the server runs each room's town itself, and its town
  is the one that is kept.

<Shot
  src="/assets/screenshots/multiplayer-host-here.webp"
  alt="The Multiplayer window: room DASJBA is open, and this computer is the server, with the address other players enter"
  caption="Hosting from the desktop game. The window shows the address the other players enter."
/>

You do not need a separate computer. Host from the desktop game with the default address. The
game starts a server itself and shows the address the other players enter. For a server
that runs all the time, there is a settings file, a health check and a Dockerfile. The
[Multiplayer guide](/guide/multiplayer) and the [server's page](/reference/server) have the
details.

## The browser version keeps your game files

The browser version used to ask for the four game files each time. It now keeps them in the
browser, on your computer only, so you choose them once. The start page then shows **Start the
game** and a manager for what it keeps.

<div class="ub-shots">
<Shot
  src="/assets/screenshots/browser-manager-mods.webp"
  alt="The browser's start page with Start the game, and the Mods tab of the manager listing four mods, two of them on"
  caption="The Mods tab: turn mods on and off, and set their order."
/>
<Shot
  src="/assets/screenshots/browser-manager-saves.webp"
  alt="The Saves tab of the browser's manager, with one save, Download and Delete buttons, and Import saves"
  caption="The Saves tab: download, delete and import saves."
/>
</div>

The manager has three tabs. **Game files** shows the kept files and can remove them. **Mods**
turns mods on and off and sets their order. **Saves** downloads and deletes saves, and imports
`.sav` files from the desktop version or the original game. To keep nothing, clear the checkbox
under **Choose files**.

## What is next

Rule hooks can only read the town and add to the event log. Next, I want to let them change the
town, at fixed points in each update, and to load them in the browser version.

## Also in 0.4.0

- **Faster loading:** a large town (a 15 MB save) loads in 536 ms instead of 827 ms.
- **Achievements in multiplayer** start from none for every player, so towns no longer reload
  because players have different achievements. Achievements you win are still saved.
- **Exit in the browser** closes the installed app, or goes back to the start page in a tab.
- **Scroll bars** fill the side of their list, as in the original.
- **The Resolution option** in the browser shows the size the game draws at.
- **WebGL 2** is used when a browser offers WebGPU but cannot draw with it.
- **Imported saves** in the browser are stored before the page reloads.
- **The probe** reads and controls a running game from the browser's console. The test suites
  use it.
- **[How Un-Banished is built](/develop/architecture)** explains the core, the extensions and
  the server, for people who work on the code.

---

**All changes:** [Changelog, version 0.4.0](/changelog) ·
**Earlier:** [Sixty a second](/blog/measuring-speed) ·
**Next:** [In Step](/blog/0-5-0-in-step)

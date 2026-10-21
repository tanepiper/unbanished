---
title: 'Harvest: Un-Banished 0.4.0'
name: Harvest
version: 0.4.0
date: 2026-10-21
cover: /assets/videos/autumn-orbit-poster.webp
summary: Mods can now change how a town plays, a server can keep your town and run it, and Pure Banished plays the original with nothing added. Plus a browser version that remembers your game files.
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
Fort Ponchartrain has 116 people, a market and a lot of yellow trees. On the first day of Autumn
this year its event log said something Banished has never said: "The town holds its harvest
festival." Nobody changed a file in the game to make it happen - a small mod did, through a door
that Un-Banished 0.4.0 opens for the first time.
</p>

## House rules

Banished mods have always changed what is in the game: new buildings, new crops, other numbers.
They could never change what the town *does*. Un-Banished 0.4.0 adds **rule hooks**: small
programs, written by mod authors, that the game runs after every update of the town. A rule hook
can look at the calendar, count the people, check the stores, roll the game's own dice - and
write to the event log.

<Shot
  src="/assets/screenshots/rule-hooks-festival.webp"
  alt="The Event Log over a town in autumn: Autumn 22, the town holds its harvest festival, and in orange, a bountiful harvest"
  caption="The example mod at work. One year in four the dice call the harvest bountiful."
/>

To use one, put its `.wasm` file in the `Hooks` folder beside your saves. Every town you start
or load picks it up, and your saves list it with the other mods.

A rule hook changes the game, so it has to change it in exactly the same way every time - on
your computer, on your friend's, and in a replay a year from now. So the game is strict with
them. A hook works with whole numbers only, it can reach nothing outside the game (no files, no
clock, no internet), and every random choice comes from the town's own dice. When it rolls them,
the roll lands in the same place on every computer. A hook that runs too long is stopped and
switched off, at the same moment everywhere.

In multiplayer, every player needs the same hooks. A player whose `Hooks` folder differs is
told so before they join, rather than finding out later in a town that slowly goes wrong.

If you want to write one, [Writing a rule hook](/guide/rule-hooks) builds the harvest festival
from an empty folder, in about sixty lines of Rust.

## Pure Banished

Everything Un-Banished adds - multiplayer, the mod sites, the probe, recordings, the film camera
and now rule hooks - is an extension, kept apart from the original game. This release made that
split real all the way down: the town itself now lives in its own part of the code, with no
drawing, sound or windows, and every extension reaches it through a few small doors.

What that gives you is a switch. Start the game with `--pure` (in the browser, add `?pure` to
the address) and you get Banished 1.0.7, with its own mods and nothing else. The Multiplayer
button goes, the Browse tabs go, and no rule hook is loaded.

<div class="ub-shots">
<Shot
  src="/assets/screenshots/main-menu-unbanished.webp"
  alt="The main menu over a snowy harbour town, with a Multiplayer button under Load"
  caption="Un-Banished as usual, with Multiplayer."
/>
<Shot
  src="/assets/screenshots/main-menu-pure.webp"
  alt="The main menu over a river town in summer, with the original's buttons only"
  caption="Pure Banished: the original's menu, and nothing more."
/>
</div>

Our tests hold us to it as well. The game builds with no extensions at all, and that build has to
play every test town to exactly the same result as the full one.

## A server of your own

In 0.3.0 a game of multiplayer lived and died with its players. The relay only passed messages
along, and when the host closed the game, the town was gone.

The relay is now a server, `unbanished-server`, and it does a lot more. Players can join a game
that has already started, and a player who drops out can come back the same way: everyone reloads
the town as it is now, so the newcomer starts from the same place as everyone else. The server checks that every player still
has the same town, and if one differs, the town most players have wins. Give the server a folder
and it keeps each room's town between evenings, so the host can open the room again next week and
carry on.

<Shot
  src="/assets/screenshots/multiplayer-host-here.webp"
  alt="The Multiplayer window: room DASJBA is open, and this computer is the server, with the address other players enter"
  caption="Hosting from the desktop game: no separate server needed."
/>

If you give it your own copy of Banished, the server goes one step further and runs every room's
town itself. It sends no orders of its own - it just keeps up with the players, update by update.
Its town is then the reference, and any player who drifts carries on from the server's.

You don't need a machine in a cupboard for any of this. Host from the desktop game with the
address left as it is: the game starts a server inside itself, then shows you the address your
friends should enter. For a server that stays up, there's a settings file, a health check and a
Dockerfile. The [Multiplayer guide](/guide/multiplayer) and the
[server's page](/reference/server) cover the details.

## The browser remembers

The browser version used to ask for your four game files every time you opened it. Now it keeps
them, in that browser only and never uploaded, so you choose them once. The start page then shows
**Start the game**, with a manager underneath for what it keeps.

<div class="ub-shots">
<Shot
  src="/assets/screenshots/browser-manager-mods.webp"
  alt="The browser's start page with Start the game, and the Mods tab of the manager listing four mods, two of them on"
  caption="Mods: switch them on and off, and set their order."
/>
<Shot
  src="/assets/screenshots/browser-manager-saves.webp"
  alt="The Saves tab of the browser's manager, with one save, Download and Delete buttons, and Import saves"
  caption="Saves: download a copy, or bring one in from the desktop."
/>
</div>

The manager has three tabs. **Game files** shows what is kept and lets you forget it. **Mods**
turns mods on and off and sets their order. **Saves** downloads your towns, deletes them, and
imports `.sav` files from the desktop version or from the original game. If you'd rather the
browser kept nothing, a checkbox under **Choose files** turns it off.

## What is next

Rule hooks can only read the town and write to the event log for now. Next I want to let them
act on it - in the same careful way, at fixed points in each update - and to load them in the
browser version as well.

## Also in 0.4.0

- **Faster loading:** a large town (a 15 MB save) loads in 536 ms instead of 827 ms.
- **Achievements in multiplayer** count from none for every player, so different achievements no
  longer make towns reload; what you win is still kept in your own settings.
- **Exit in the browser** closes the installed app, or goes back to the start page in a tab.
- **Scroll bars** fill the side of their list, as in the original.
- **The Resolution option** in the browser shows the size the game really draws at.
- **WebGL 2** takes over when a browser offers WebGPU but cannot draw with it.
- **Imported saves** in the browser are written before the page reloads.
- **The probe** lets you look at and drive a running game from the browser's console, and our
  test suites use it.
- **[How Un-Banished is built](/develop/architecture)** explains the core, its extensions and
  the server, for anyone who wants to work on the code.

---

**All changes:** [Changelog, version 0.4.0](/changelog) ·
**Earlier:** [Sixty a second](/blog/measuring-speed)

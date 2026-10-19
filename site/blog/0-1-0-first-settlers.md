---
title: 'First Settlers: Un-Banished 0.1.0'
name: First Settlers
version: 0.1.0
date: 2026-10-19
cover: /assets/videos/zoom-into-town-poster.webp
summary: The first release. The whole of Banished runs on a new engine, on Windows, macOS, Linux and in your browser, from your own copy of the game.
description: Un-Banished 0.1.0, the first release. The whole of Banished runs on a new engine written in Rust, natively and in the browser.
outline: [2, 2]
---

<ReleaseHero
  name="First Settlers"
  version="0.1.0"
  date="2026-10-19"
  video="/assets/videos/zoom-into-town.webm"
  poster="/assets/videos/zoom-into-town-poster.webp"
  alt="The camera descends from high above a town in a forest to its market"
/>

<p class="ub-lead">
A few families arrive in a valley with a cart of supplies, and the winter is coming. That is
how every game of Banished starts. It is also how this project started: with an empty folder
and a copy of the finished game. Version 0.1.0 is the first time the whole game runs on its new
engine, on your computer or in your browser.
</p>

## A town you already know

Un-Banished is a new engine for Banished, written from scratch in Rust. It contains none of the
original's code, art, sound or text. Instead, it reads the game files from your own copy of
Banished, the same files that the original reads. So everything looks, sounds and plays as you
remember it.

<Shot
  src="/assets/screenshots/town-overview.webp"
  alt="A town of houses, fields and an orchard in a forest"
  caption="A town loaded from a save of the original game."
/>

## Same seed, same valley

When you start a new game, Banished makes the valley from a random number, the seed. Un-Banished
makes exactly the same valley from the same seed: the same hills, the same rivers, the same
trees and stones, to the last bit. We checked this by running the original's own map functions
in an emulator and comparing every value.

<div class="ub-shots">
<Shot
  src="/assets/screenshots/new-game-dialog.webp"
  alt="The New dialog with the settings of a new game"
  caption="Choose the terrain, the climate and the difficulty, as in the original."
/>
<Shot
  src="/assets/screenshots/new-game.webp"
  alt="The first settlers next to their stockpile in a new valley"
  caption="The first minute of a new town."
/>
</div>

## Every citizen, every chicken

The citizens live their whole lives: they are born, grow up, start families, work, eat, get
cold, get sick and grow old. They farm fields and orchards, herd animals, hunt, fish, gather, build roads
and bridges, and trade with the merchants who come up the river. The animals are there too,
every chicken with its walk.

<div class="ub-shots">
<Shot
  src="/assets/videos/citizens-market.webm"
  poster="/assets/videos/citizens-market-poster.webp"
  alt="Citizens carry goods to and from the market stalls"
  caption="Market day."
/>
<Shot
  src="/assets/videos/chicken-walk.webm"
  alt="A chicken walks on the spot"
  caption="The chicken's walk, from the original's animation."
/>
</div>

## A year in a few seconds

The seasons turn as they did: the fields go golden, the snow comes, and the town waits for
spring. Speed up the game, and the year flies past.

<Shot
  src="/assets/videos/seasons.webm"
  poster="/assets/videos/seasons-poster.webp"
  alt="A town changes from autumn to winter to spring"
  caption="Autumn, winter and spring at speed 10."
/>

## Your old towns still live here

Un-Banished uses the original save format and the original save folder. A town that you saved
in the original game years ago loads in Un-Banished, with its citizens, its stores and its
history. The menus, the toolbar and every window are the original's, drawn by the new engine.

<Shot
  src="/assets/screenshots/town-hall.webp"
  alt="The Town Hall window open on its Inventory tab"
  caption="The Town Hall, with the stock of a town from an old save."
/>

## In your browser

The same engine runs in a web browser, through WebAssembly. Open the
[Play page](/play), choose the four game files from your copy of Banished, and play. The files
stay in your browser. Saves, settings and mods are kept there too.

## Built from evidence

We did not have the source code of Banished. We had the finished program. So every part of
Un-Banished started with evidence: we read the original's machine code with Ghidra, ran its
functions in an emulator to get exact results, and then wrote new code that gives the same
results. About 1,600 original functions are named in the code next to the new code that
replaces them. The whole story is in [How it was made](/about/how-it-was-made).

## Fixed before the release

Players found the first problems quickly, and we fixed them before the release:

- A construction site could not be selected, and an orchard could not be selected once its
  trees had grown.
- Changes in building windows did not show while the game was paused.
- After loading a save, walking citizens stood still.
- The browser version ran out of memory in Firefox.
- The game paused for a moment every few seconds.

## Known issues

There are no ready-made downloads yet: you build Un-Banished from its source code (see the
[setup guide](/guide/)). A few things still differ from the original; they are listed in the
[issues](https://github.com/tanepiper/unbanished/issues).

---

**All changes:** [Changelog, version 0.1.0](/changelog) ·
**Next:** [Boomtown, 0.2.0](/blog/0-2-0-boomtown)

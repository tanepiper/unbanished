---
title: 'Sixty a second: how we measure Un-Banished'
name: Sixty a second
kind: Behind the scenes
date: 2026-10-20T22:00:00Z
cover: /assets/videos/thaw-at-speed-ten-poster.webp
summary: Does Un-Banished draw 60 frames a second? We measured four towns, from a new game to 1,024 citizens, in the browser and on the desktop. Here is how we test, what we found, and where the next gains are.
description: How Un-Banished measures its speed in the browser and on the desktop, the frame rates of four towns at three game speeds, where the time goes, and where the game can still become faster.
outline: [2, 2]
---

<ReleaseHero
  name="Sixty a second"
  kind="Behind the scenes"
  date="2026-10-20T22:00:00Z"
  video="/assets/videos/thaw-at-speed-ten.webm"
  poster="/assets/videos/thaw-at-speed-ten-poster.webp"
  alt="A winter town by a river at ten times speed: the snow melts, the river opens and the fields turn green in a few seconds"
/>

<p class="ub-lead">
A city builder should feel smooth: 60 pictures a second, even when the town is large and the game
runs fast. Does Un-Banished manage that? We measured four towns at three game speeds, in a
browser and on the desktop. This post shows how we measure, what we found, and where the game
can still become faster. And every change we make to go faster must leave the game exactly as it
was, to the last bit.
</p>

## Does it reach 60?

At normal speed, yes: every town we tested holds 60 frames a second in the browser. At 5× and 10×
speed, the small towns still do, but the large towns fall behind.

We used four towns. Testwood is a new game (seed 1234) with 19 people. Fort Ponchartrain has 116
people. Goldfoothill is a player's town in its 83rd year, with 565 people. Driscola, built in the
original game in 2015, has 1,024 people, 650 buildings and about 28,000 objects.

<FpsGrid
  caption="In the browser: Chrome with WebGPU, in a 1280 × 720 page, on a MacBook Pro with an Apple M3. Real frames, counted for 20 seconds. A slow frame takes more than 20 ms."
  :rows="[
    { town: 'Testwood', detail: '19 people', cells: [{ fps: 60, note: 'smooth' }, { fps: 60, note: 'smooth' }, { fps: 60, note: 'smooth' }] },
    { town: 'Fort Ponchartrain', detail: '116 people', cells: [{ fps: 60, note: 'smooth' }, { fps: 58.8, note: '2% slow frames' }, { fps: 60, note: 'smooth' }] },
    { town: 'Goldfoothill', detail: '565 people', cells: [{ fps: 59.8, note: 'smooth' }, { fps: 56, note: '7% slow frames' }, { fps: 28.4, note: '84% slow frames' }] },
    { town: 'Driscola', detail: '1,024 people', cells: [{ fps: 59.6, note: 'smooth' }, { fps: 26.1, note: '78% slow frames' }, { fps: 21.6, note: '87% slow frames' }] },
  ]"
/>

Firefox draws with WebGL 2 instead of WebGPU. In Goldfoothill it gets 58 frames a second at
normal speed, 28 at 5× and 23 at 10×.

On the desktop the game is faster. The first number is the slowest case, where the processor
and the graphics card take turns. In a window they work side by side, and the second number is
that case.

<FpsGrid
  caption="On the desktop: the same Mac, 1920 × 1080, measured by the game's own frame profiler. A monitor shows at most its refresh rate, usually 60."
  :rows="[
    { town: 'Testwood', detail: '19 people', cells: [{ fps: 289, note: 'up to 563 in a window' }, { fps: 220, note: 'up to 397 in a window' }, { fps: 167, note: 'up to 316 in a window' }] },
    { town: 'Fort Ponchartrain', detail: '116 people', cells: [{ fps: 125, note: 'up to 209 in a window' }, { fps: 81, note: 'up to 144 in a window' }, { fps: 72, note: 'up to 117 in a window' }] },
    { town: 'Goldfoothill', detail: '565 people', cells: [{ fps: 80, note: 'up to 119 in a window' }, { fps: 60, note: 'up to 83 in a window' }, { fps: 44, note: 'up to 60 in a window' }] },
    { town: 'Driscola', detail: '1,024 people', cells: [{ fps: 58, note: 'up to 86 in a window' }, { fps: 40, note: 'up to 56 in a window' }, { fps: 26, note: 'up to 40 in a window' }] },
  ]"
/>

So 7 of the 12 browser tests reach 60, and 10 of the 12 desktop tests do in a window. What is
left is the large towns at high speed.

## How we measure

We measure in two ways, because they answer two different questions.

**Real frames, in a real browser.** A test script starts Chrome or Firefox, opens the game,
gives it the game files and a save, and presses the buttons a player would press. It drives the
game through the [probe](/develop/probe), a small command language that finds buttons and reads
the town. When the town is running, the page counts every frame it shows for 20 seconds. That
gives the frame rate a player sees, and how many frames are slow.

**Where the time goes, on the desktop.** The game has its own frame profiler. When it is on, it
times each part of every frame: the simulation, the citizens, the models, the sound, the
particles, the drawing on the processor and the work of the graphics card. Every 600 frames it
writes a summary. We run it without a window, with the same save and the same script each time,
so two runs can be compared. To see which functions take the time, we also use a sampling
profiler, [samply](https://github.com/mstange/samply).

**Faster, but the same game.** Un-Banished must play exactly like Banished. So a change that
makes the game faster must not change anything a player can see: not one save, not one frame,
not one random number. Before a speed-up goes in, a tool builds the old and the new version,
plays the same towns in both, and compares every save and every picture. If one byte differs,
the change does not go in. [Compatibility between versions](/develop/compatibility) explains how
it works.

## Where the time goes

Driscola is the hardest test, so we profiled it at normal speed and at 10× speed. A frame at 60
frames a second has 16.7 ms for one step of the game and one picture. These are the times on the
desktop:

| Part of a step | Normal speed | 10× speed |
|---|---|---|
| The simulation | 1.8 ms | 15.9 ms |
| of which the citizens | 1.2 ms | 11.7 ms |
| Refreshing what you see: models, sound, smoke and dust | 5.7 ms | 5.5 ms |
| Preparing the picture (processor) | 4.3 ms | 4.1 ms |
| Drawing the picture (graphics card) | 9.9 ms | 15.3 ms |

Three things stand out:

- **At 10× speed, the simulation alone fills a frame.** The game runs ten steps of the town in
  each frame, and most of that time is the citizens deciding what to do and finding their way.
- **At normal speed, refreshing what you see costs three times the simulation.** Every step, the
  game rebuilds the list of models to draw, updates the sounds around the camera and moves the
  smoke and dust. It does this whatever the speed, and most of it has not changed since the last
  step.
- **The browser feels it most.** In the browser the game runs on one processor core, in
  WebAssembly, which is slower than native code. On the desktop, loading already uses every core.

## Where we can go faster

These are the gains we have found, with an estimate of each from the profiles. Each one must
pass the same test: the same saves and the same pictures as before.

- **Citizens who remember.** Every step, each citizen's view of the town's buildings and stock
  is built again from the start, although most of it has not changed. Keeping those views, and
  updating only what changed, would save about a tenth of the processor's time. The order in
  which the citizens think must stay exactly the same, so this one needs care.
- **Rebuild the picture only where the town changed.** The list of models to draw is built
  again every step, grouped by name, and sent to the graphics card again: together about a
  quarter of the processor's time. Most buildings do not move. We tried to rebuild only the
  changed groups once, but growing trees change almost every group a little every step. A
  cheaper way to know what changed is the next attempt. The sounds and the smoke can follow the
  same idea.
- **Two things at once.** The simulation must stay in one line, step after step, so that every
  computer gets the same town. But the work around it, such as refreshing the models and
  preparing the picture, can run on another core while the next step of the town runs.
- **Threads in the browser.** A web page can use more than one core only when the server sends
  two special headers, and the site's host cannot send them. The browser version can now be
  installed as an app, and its service worker can add those headers itself. Then the browser can
  use the same parallel loading as the desktop.
- **A lighter load on the graphics card.** The game already skips everything outside the view.
  The ground decals and the sun's shadows are the next largest costs, and can be culled in the
  same way.
- **Smaller wins.** A faster method for the game's internal lookup tables (about 6 % of the
  processor's time), and less copying of lists from frame to frame.

We will measure again after each of these, with the same four towns and the same scripts, and
show the results here.

---

**Details:** [Performance](/develop/performance), with every measurement ·
[Compatibility between versions](/develop/compatibility) ·
**Earlier:** [Neighbours, 0.3.0](/blog/0-3-0-neighbours)

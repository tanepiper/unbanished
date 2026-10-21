---
title: 'Boomtown: Un-Banished 0.2.0'
name: Boomtown
version: 0.2.0
date: 2026-10-19
cover: /assets/videos/big-town-poster.webp
summary: Towns of a thousand citizens run twice as fast, the GPU does half the work, and mods arrive from Nexus Mods. Not one save or frame of the game changes.
description: Un-Banished 0.2.0 makes large towns run twice as fast, halves the work of the GPU, adds a mod manager and Nexus Mods, and gives the browser version a new front door.
outline: [2, 2]
---

<ReleaseHero
  name="Boomtown"
  version="0.2.0"
  date="2026-10-19"
  video="/assets/videos/big-town.webm"
  poster="/assets/videos/big-town-poster.webp"
  alt="The camera circles the church and graveyard of Driscola, a town of 1,024 citizens, between rows of houses and an autumn wood"
/>

<p class="ub-lead">
Every town in Banished starts with a few families and a pile of supplies. Some towns grow into
cities of a thousand people. That is where the original game starts to struggle. Version
0.2.0 is about those big towns. They now run twice as fast. They play exactly the same as before,
with the same saves and the same frames.
</p>

## Big towns, twice as fast

Driscola is a real town. A player built it in the original game in 2015. It has 1,024 citizens,
448 wild animals and about 28,000 objects: trees, houses, fields, fences, stones and piles of
goods. The film at the top of this page circles its church. We use Driscola as our test of
speed.

At the highest game speed, one frame of Driscola took 94 milliseconds in Un-Banished 0.1.0.
That is about ten frames a second. Most of the time went to work that the game did again and
again for no reason. For example, every citizen looked at every object in the town about twice
in each step of the simulation. Now the game remembers what it already knows, and works out
again only what has changed.

The GPU had the same problem. A frame is drawn three times: the sun's view, for the shadows; the
town upside down, for the reflection in the water; and the view that you see. In 0.1.0 every
tree and every house was drawn in all three, even far outside the picture. Now each part of the
picture gets only the trees, houses and pieces of ground that it can see.

<BeforeAfter
  unit="ms"
  :rows="[
    { label: 'One frame of Driscola at speed 10', before: 94, after: 43 },
    { label: 'The simulation part of that frame', before: 78, after: 29 },
    { label: 'GPU time for one frame of Driscola', before: 32, after: 17 },
  ]"
/>

<Shot
  src="/assets/videos/orbit.webm"
  poster="/assets/videos/orbit-poster.webp"
  alt="The camera circles slowly around a market full of coloured stalls"
  caption="Every building that turns out of view is no longer drawn. You cannot see the difference. That is the point." />

Loading is faster too. On a computer with more than one processor core, the game's pictures,
sounds and models are now decoded on all the cores at the same time, so starting the game and
loading a save take 20 to 55 % less time. And when a town appears after loading, it no longer
stops for half a second to prepare its sounds: that now happens behind the loading screen.

How do we know that nothing changed? Un-Banished must play exactly like Banished. Every new
version must play exactly like the one before it, bit for bit. A new tool, `tools/compat`,
builds two versions side by side, plays the same towns in both and compares every saved game
and every picture. It also lets each version continue the other's saves. Every change in 0.2.0
passed it with identical results. [Compatibility between versions](/develop/compatibility)
explains how it works.

## Mods: a manager, and Nexus Mods

Banished has a large mod community, and many mods are on Nexus Mods. There is now a mod manager,
`banished-mods`. In it, you can turn mods on and off, set their order, keep named lists of mods
and start the game.

If you have a Nexus Mods account, you can also give the game your own Nexus Mods API key. Then
the Mods menu's Browse tab (where the Steam version shows the Workshop) searches Nexus Mods for
Banished mods and installs them. The same works from the command line with `banished-cli nexus`.
See [Mods](/guide/mods) in the guide.

<Shot
  src="/assets/screenshots/mod-manager.webp"
  alt="The mod manager with five mods, three of them on, and the details of Better Fields"
  caption="The mod manager: five mods, three on, and the details of the selected mod."
/>

## People at work, up close

Every film in this post was recorded by Un-Banished itself, without a window: the game runs, a
script moves the camera. Each frame is saved and joined into a film. The game's own camera
stays high above the town, as in the original, so for this release the recordings got a camera
of their own. It can fly along any path, or ride behind a citizen, a boat or an animal and look
where they are going. When a roof or a tree comes between the camera and its subject, it moves
closer or rises, so the view stays clear.

<Shot
  src="/assets/videos/river-flight.webm"
  poster="/assets/videos/river-flight-poster.webp"
  alt="The camera flies low down a river and into a town"
  caption="A flight down the river and into the town."
/>

With it, for the first time, you can see the people of a Banished town at work, close enough to
see what they carry, and ride with the trader as the boat rows home.

<div class="ub-shots">
<Shot
  src="/assets/videos/boat-shoulder.webm"
  poster="/assets/videos/boat-shoulder-poster.webp"
  alt="Over the merchant's shoulder, the trader's boat rows down the river, under a bridge, towards the trading post"
  caption="Over the merchant's shoulder, down the river to the trading post."
/>
<Shot
  src="/assets/videos/farmer.webm"
  poster="/assets/videos/farmer-poster.webp"
  alt="A farmer harvests a field next to the market and carries the crop away"
  caption="A farmer brings in the harvest."
/>
<Shot
  src="/assets/videos/vendor.webm"
  poster="/assets/videos/vendor-poster.webp"
  alt="A vendor walks among the coloured market stalls"
  caption="A vendor among the market stalls."
/>
<Shot
  src="/assets/videos/fisher-closeup.webm"
  poster="/assets/videos/fisher-closeup-poster.webp"
  alt="The camera circles a fisher with a rod at the end of a dock"
  caption="A fisher at the end of the dock."
/>
</div>

Anyone can make films like these with `tools/videos.sh`; [Recording videos](/develop/videos)
explains how.

## A new front door for the browser

The browser version now opens with its own loading screen. It explains, in three steps, where to
find the game files on your computer and which four to choose. The files stay in your browser;
nothing is uploaded.

<Shot
  src="/assets/screenshots/browser-front-door.webp"
  alt="The browser loading screen: the Un-Banished logo and three numbered steps"
  caption="The browser version's loading screen. Try it on the Play page."
/>

[Play in your browser](/play)

## What is next

The work on large towns continues: the decals and the shadows are the next largest costs on the
GPU. If something does not play like the original, please
[report it](https://github.com/tanepiper/unbanished/issues/new/choose).

## Also in 0.2.0

- **Rocks and ore** far from the camera use their simpler model, as in the original.
- **The trader's boat** is drawn on the river, with its hull, cargo, oars and merchant.
- **Smaller pieces of code:** the exact maths, the terrain and the path finding are now their own
  libraries (`banished-core`, `banished-terrain`, `banished-path`). The largest files are split
  by topic. The game does not change.
- **More documentation:** the website has new [Guide](/guide/), [Develop](/develop/),
  [Topics](/topics/) and [Reference](/reference/) sections, every app and library has a README,
  and the Rust API documentation is written in plain English.
- **A blog**, with the story of each release.

---

**All changes:** [Changelog, version 0.2.0](/changelog) ·
**Earlier:** [First Settlers, 0.1.0](/blog/0-1-0-first-settlers) ·
**Next:** [Neighbours, 0.3.0](/blog/0-3-0-neighbours)

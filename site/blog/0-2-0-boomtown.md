---
title: 'Boomtown: Un-Banished 0.2.0'
name: Boomtown
version: 0.2.0
date: 2026-10-19
cover: /assets/videos/orbit-poster.webp
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
  alt="The camera turns over Driscola, a town of 1,024 citizens, at the highest game speed. Smoke rises from many chimneys and from a fire."
/>

<p class="ub-lead">
Every town in Banished starts with a few families and a pile of supplies. Some towns grow into
cities of a thousand people, and that is where the original game starts to struggle. Version
0.2.0 is about those big towns. They now run twice as fast, and they look exactly the same as
before: not one save, not one frame of the game has changed.
</p>

## Driscola, 1,024 citizens

Driscola is a real town. A player built it in the original game in 2015. It has 1,024 citizens,
448 wild animals and about 28,000 objects: trees, houses, fields, fences, stones and piles of
goods. It is also on fire.

We use Driscola as our stress test. At the highest game speed, one frame of Driscola took
94 milliseconds in Un-Banished 0.1.0. That is about ten frames a second. Most of the time went
to work that the game did again and again for no reason. For example, every citizen looked at
every object in the town about twice in each step of the simulation. In each step, the game also
compared the whole 512 × 512 map about six times, to see if anything had changed.

Now the game remembers what it already knows. The citizens' view of the town is built only for
objects that matter to them, and the map keeps a counter that goes up when something changes.

<BeforeAfter
  unit="ms"
  :rows="[
    { label: 'One frame of Driscola at speed 10', before: 94, after: 43 },
    { label: 'The simulation part of that frame', before: 78, after: 29 },
  ]"
/>

The video at the top of this page is Driscola at speed 10, recorded by Un-Banished itself.

## The GPU does half the work

A frame is drawn three times. First, the sun's view, to make the shadows. Second, the town
upside down, for the reflection in the water. Third, the view that you see. In 0.1.0, every
tree and every house was drawn in all three, even when it was far outside the picture.

We measured each part of the frame and found the biggest cost in an unexpected place: the
ground under the water. Every piece of terrain was drawn into the reflection, and the water
shader then threw most of it away. Now each part of the picture gets only the trees, houses
and pieces of ground that it can see. In the view that you see, that is about 2,700 of
Driscola's 25,800 models.

<BeforeAfter
  unit="ms"
  :rows="[
    { label: 'GPU time for one frame of Driscola', before: 32, after: 17 },
  ]"
/>

<Shot
  src="/assets/videos/orbit.webm"
  poster="/assets/videos/orbit-poster.webp"
  alt="The camera circles slowly around a market full of coloured stalls"
  caption="Every building that turns out of view is no longer drawn. You cannot see the difference, and that is the point."
/>

## Proof that nothing changed

Un-Banished must play exactly like Banished, and every new version must play exactly like the
one before it. "Exactly" means bit for bit. So before we changed anything, we built a tool to
check it.

`tools/compat` builds any two versions of Un-Banished side by side. It plays the same towns in
both: a new game, a small town, a town of 116 citizens and Driscola. Then it compares every
saved game and every picture. It also lets each version continue the other's saves. Every
change in 0.2.0 passed this test with identical results.

Once, the test found a difference. The cause was not the change, but the test itself: the save
was written a few frames later in one run than in the other. We fixed the test. This story is
in [Compatibility between versions](/develop/compatibility).

## Rocks and ore, the way they were

In tracing how the original draws its models, we found one small thing that we did wrong. Three
rocks and three ore deposits have a second, simpler model, used when the camera is more than 50
to 70 units away. Un-Banished always used the detailed one. Now it switches, as the original
does.

<Shot
  src="/assets/screenshots/visual-change/rocks.webp"
  alt="Three pictures of the same rocks: before, after, and the few changed pixels marked in red"
  caption="Before, after, and the difference. Only the edges of the rocks change; nothing else in the picture does."
/>

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

## A new front door for the browser

The browser version now opens with its own loading screen. It explains, in three steps, where to
find the game files on your computer and which four to choose. The files stay in your browser;
nothing is uploaded.

<Shot
  src="/assets/screenshots/browser-loading.webp"
  alt="The browser loading screen: the Un-Banished logo and three numbered steps"
  caption="The browser version's loading screen. Try it on the Play page."
/>

[Play in your browser](/play)

## No freeze after loading

When a town appeared after loading, the game used to stop for about half a second. It was
decoding the sounds of the town and the first piece of music, at the moment you wanted to play.
Now this happens behind the loading screen.

Loading itself is also faster on computers with more than one processor core. The game's
pictures, sounds and models are now decoded on all the cores at the same time. Starting the game
and loading a save take 20 to 55 % less time. (The browser version still loads on one core.)

## The camera learns new tricks

All the videos in this post were recorded by Un-Banished itself, without a window. For this
release, our recording tool learned to hold down camera keys, to turn and tilt the camera, and to
follow a citizen.

<div class="ub-shots">
<Shot
  src="/assets/videos/follow-citizen.webm"
  poster="/assets/videos/follow-citizen-poster.webp"
  alt="The camera follows one citizen as they walk between houses"
  caption="Following one citizen through the town."
/>
<Shot
  src="/assets/videos/river.webm"
  poster="/assets/videos/river-poster.webp"
  alt="The camera flies low along a river past a bridge and forest"
  caption="Along the river from the bridge."
/>
<Shot
  src="/assets/videos/fishing.webm"
  poster="/assets/videos/fishing-poster.webp"
  alt="The camera turns slowly around a fishing dock on a river"
  caption="A fishing dock, close up."
/>
<Shot
  src="/assets/videos/seasons.webm"
  poster="/assets/videos/seasons-poster.webp"
  alt="A town changes from autumn to winter to spring"
  caption="A year at speed 10."
/>
</div>

## Under the floorboards

The code is now in smaller pieces. The exact maths, the terrain and the path finding are now
their own libraries: `banished-core`, `banished-terrain` and `banished-path`. The largest files
are split by topic. None of this changes the game, and the compatibility test proves it.

The documentation grew too. The website has new [Guide](/guide/), [Develop](/develop/),
[Topics](/topics/) and [Reference](/reference/) sections. Every app and library has a README,
and the Rust API documentation is written in plain English.

## What is next

The work on large towns continues: the decals and the shadows are the next largest costs on the
GPU. We also want to find out why a few frames can differ between two runs of the same version
(the saves never do).

If something does not play like the original, please
[report it](https://github.com/tanepiper/unbanished/issues/new/choose).

---

**All changes:** [Changelog, version 0.2.0](/changelog) ·
**Earlier:** [First Settlers, 0.1.0](/blog/0-1-0-first-settlers)

---
title: 'Neighbours: Un-Banished 0.3.0'
name: Neighbours
version: 0.3.0
date: 2026-10-20
cover: /assets/videos/multiplayer-poster.webp
summary: Several players can now build one town together, on the desktop or in the browser. Plus mods from banishedinfo.com, a browser version you can install, and a fix for the first tutorial.
description: Un-Banished 0.3.0 adds multiplayer, mods from banishedinfo.com, a browser version that installs as an app, and a fix for the first tutorial.
outline: [2, 2]
---

<ReleaseHero
  name="Neighbours"
  version="0.3.0"
  date="2026-10-20"
  video="/assets/videos/multiplayer.webm"
  poster="/assets/videos/multiplayer-poster.webp"
  alt="Two games side by side, on two computers, show the same town as two players build in it"
/>

<p class="ub-lead">
Banished was always a quiet game for one: a handful of families, a long winter, and you. In
Un-Banished 0.3.0 your town can have neighbours. Several players can now build one town together,
each on their own computer, or in a browser. And the single-player game is still exactly the
game it was.
</p>

## One town, many hands

Open **Multiplayer** on the main menu. One player hosts a room and gets a short code; the others
type the code and join. The host picks a save, and everybody's game loads the same town. From
then on, every player can build, clear, give orders, change the speed and pause, and everybody
sees it happen.

<div class="ub-shots">
<Shot
  src="/assets/screenshots/multiplayer-host.webp"
  alt="The Multiplayer window over the main menu, with room code HQVTRM open for two players"
  caption="The host's window: the room code to give the others, and Start."
/>
<Shot
  src="/assets/screenshots/multiplayer-join.webp"
  alt="The Multiplayer window of a player who has joined room HQVTRM and waits for the host"
  caption="A player who joined: in the room, waiting for the host to start."
/>
</div>

In the game, a line at the foot of the screen says which player you are. When the game waits
for a slower computer, it names the player, and you can still move the camera and use the
windows meanwhile.

<Shot
  src="/assets/screenshots/multiplayer-players-line.webp"
  alt="The players line at the foot of the screen: Multiplayer, Player 1 of 2, the host"
  caption="The players line."
/>

Every player needs their own copy of Banished, the same mods and the same version of
Un-Banished. The relay that connects the players comes with the game: one player, or a small
server, runs it.

The [Multiplayer guide](/guide/multiplayer) explains every step, from running the relay to
playing over the internet.

## Nobody sends the town

A town of a thousand citizens changes thousands of times a second. Sending all of that over the
internet is not possible. So the players' computers send each other only what the players *do*:
"place a house here", "two more farmers", "faster". Every computer then runs the whole town
itself.

This only works if every computer computes exactly the same town from the same orders, to the
last bit. Un-Banished could already do that: it was built to match the original game bit for
bit. So we first made every player action into one kind of order, a command, without changing
the single-player game at all. Then we checked it: a recording of the commands plays back to
the same town; three simulated players on the 1,024-citizen town agree at every one of 61
checks over 3,600 updates; and the desktop and the browser compute the same town
through a full game year.

The idea is the same as in the Banished Multiplayer mod for the original game: every player runs
the simulation, and only the commands travel.

## When the towns drift apart

If something ever makes one computer's town differ from the others, the game notices. About every ten
seconds the players compare a short fingerprint of their towns. If one differs, the host saves
its town and every player loads it, and the game goes on from there, a moment later, in step
again. If a player leaves, the others play on.

## Mods from banishedinfo.com

Banished has had a modding community for ten years, and many of its mods live on
banishedinfo.com. On the desktop, the game's Mods menu now has a Browse tab for it, next to the
one for Nexus Mods. You can search, read about a mod, see its picture and install it, in the
game, in the mod manager or from the command line.

<div class="ub-shots">
<Shot
  src="/assets/screenshots/mods-browse-banishedinfo.webp"
  alt="The game's Mods window on its Browse banishedinfo.com tab: a grid of mods with their pictures"
  caption="In the game: Mods, then Browse banishedinfo.com. The plus button installs a mod."
/>
<Shot
  src="/assets/screenshots/mod-manager-banishedinfo.webp"
  alt="The mod manager's banishedinfo.com tab: a list of mods with Install newest and Versions buttons"
  caption="In the mod manager: search, sort, and install the newest version or an older one."
/>
</div>

No account is needed to browse or install from banishedinfo.com. The site does not allow other
web pages to read it, so this tab is on the desktop only; the browser version keeps the Nexus
Mods tab.

## The browser version grows up

The browser version now behaves more like a real app:

- **Install it.** Chrome and Edge offer to install Un-Banished. It then opens in its own window
  and starts without an internet connection. Your game files, saves and mods stay in the browser.
- **Full screen** from the game's Options, and from the Play page's Full screen button.
- **Resolution and frame rate.** Draw the 3D world at 100, 75 or 50 % of the page, and limit
  the frame rate to between 30 and 120 per second. Slow laptops, tablets and phones run more
  smoothly; the menus stay sharp.
- **Sharper text** on 4K screens and tablets, where letters used to fall between pixels.

<Shot
  src="/assets/screenshots/browser-loading.webp"
  alt="The browser version's loading screen with its three steps"
  caption="The browser version's front door. It now installs as an app."
/>

## Our first bug report

Shortly after 0.2.0, a new player told us that the first tutorial stopped at its first house:
the Wood House button stayed grey. They were right, and every version before this one had the
same problem, in the browser and on the desktop. The tutorial waited for a click on the
Housing menu, but the toolbar's menu buttons send a different kind of event, so the tutorial
never saw it. It is fixed, and a test now plays that step the way a player does. Thank you for
the [report](https://github.com/tanepiper/unbanished/issues/11), and for the four other ideas
that came with it.

## What is next

Next we want to look at how people and boats move past each other: in Banished they walk
through one another, and boats sail through bridge supports. That is how the original works, so
any change will go to a [community vote](https://github.com/tanepiper/unbanished/issues/10)
first.

---

**All changes:** [Changelog, version 0.3.0](/changelog) ·
**Earlier:** [Boomtown, 0.2.0](/blog/0-2-0-boomtown)

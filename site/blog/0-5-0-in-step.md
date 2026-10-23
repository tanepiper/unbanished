---
title: 'In Step: Un-Banished 0.5.0'
name: In Step
version: 0.5.0
date: 2026-10-23
cover: /assets/videos/seminola-banner-poster.webp
summary: A town now works through its people, buildings, trees and animals in the same order as Banished, a new game starts the same way, and daily life follows Banished's own rules.
description: Un-Banished 0.5.0 runs each update of the town in Banished's own order, starts a new game the same way as Banished, and follows Banished's rules for illness, food, carrying and animation.
outline: [2, 2]
---

<ReleaseHero
  name="In Step"
  version="0.5.0"
  date="2026-10-23"
  video="/assets/videos/seminola-banner.webm"
  poster="/assets/videos/seminola-banner-poster.webp"
  alt="The camera circles a young town: a log house, an animal pen and a stockpile of stone on open grass"
/>

<p class="ub-lead">
Un-Banished 0.5.0 makes a town behave more like it does in Banished. Each update goes through
the town in the same order as Banished, a new game starts the same way, and everyday rules such
as how an illness spreads now give Banished's results.
</p>

## In step with Banished

Many times a second, the game updates the town. Each time, it goes through the people, the
buildings, the trees and the animals, one after another. Banished does this in a fixed order: the
things that change every update come first, then the people, then the wild animals' herds, then
the buildings and piles, then the trees, crops and animals. Un-Banished now uses the same order.

The order matters more than it sounds. Banished draws many of its decisions from one stream of
random numbers - who falls ill, or how fast someone walks. If two things take
their turn in a different order, they take different numbers from that stream. From there, the
town goes its own way.

<Shot
  src="/assets/videos/goldfoothill-walk.webm"
  poster="/assets/videos/goldfoothill-walk-poster.webp"
  alt="The camera follows a citizen through a large town in snow, past a trading dock with a boat on the river"
  caption="A large town, each of its people taking their turn in Banished's order."
/>

Anything that is gone, such as a building you tore down or a pile someone picked up, leaves the
order at once, as it does in Banished. So a town you have played for years still updates every
house, tree and deer as often as a new one does.

## A new game, the same as Banished's

I have two saves that Banished made a moment after a new game started. Un-Banished can now start
the same map with the same settings and reach the same point. The trees, the families, the deer
and all sixteen herds come out the same. All seventeen citizens are the same down to the
smallest detail - where they stand, which way they face, what they are doing and how far through
their step they are.

<Shot
  src="/assets/videos/new-game-orbit.webm"
  poster="/assets/videos/new-game-orbit-poster.webp"
  alt="The camera circles the start of a new game in light rain: a barn, a stockpile and the first families"
  caption="The start of a new game. Banished's own save of this moment and Un-Banished's now match."
/>

Getting there meant matching many small things. Deer get their ages and sexes as in Banished.
Herds settle where Banished puts them. The starting adults count as having finished school.

## Daily life by Banished's rules

The everyday rules of a town now follow Banished's more closely. An illness passes to one
neighbour at a time, so it spreads more slowly than before. A hungry citizen eats the food at
home first, and only walks to a store when that is not enough. Vendors and traders carry 500
goods at a time. Tools and farm animals make
their sounds again - the ring of the blacksmith's anvil, the chickens in their pen.

When you load a game, every person and animal carries on as they were. A citizen halfway
across the square keeps walking from the same step, in time with the others.

## How we check our work

Un-Banished is built with [GameFrame](https://github.com/tanepiper/game-frame), a set of rules,
tools and AI helpers for rebuilding old games exactly. The helpers follow the same steps each
time: find out how Banished does something, run that part of Banished, write the same thing in
Un-Banished, and prove the two give the same answer.

The most important of those steps is running Banished's own program, not only reading it. We
run a piece of Banished with thousands of inputs and compare its answers with ours to the last
digit. In this release that covered the random numbers, a citizen's health and happiness, how
fruit ripens and how the seasons turn. Reading alone was not enough - several rules that looked
right when read gave different answers in rare cases once they were run.

GameFrame also keeps the work safe to share. It stops anyone, person or AI, from adding
Banished's own files or code to the project, so Un-Banished holds only its own code and plays
from your own copy of Banished. Every comparison is a test that anyone with Banished can run
again, and what we learn goes back into GameFrame for other games.

## What is next

One part of the order is still open: within the first group of each update, Banished sorts
things in a way a port cannot copy directly, and choosing the closest match needs two more saves
from Banished. After that, I want to check how citizens find a home against a longer game of
Banished.

## Also in 0.5.0

- **Walkers start on time:** a citizen or animal sets off one update after asking for a path,
  as in Banished.
- **Deer and citizens keep their heading** when they stand still, instead of turning north.
- **Saves keep more:** fishing docks keep each fisher's place, and saves from 2015 load
  citizens who were eating from their own hands.
- **Animation is saved:** saves now hold every character's animation and the clocks that walks
  share.
- **Traders' names:** merchants get men's names for men and women's names for women.
- **Tests are silent:** the tests and tools never play the game's sound.

---

**All changes:** [Changelog, version 0.5.0](/changelog) ·
**Earlier:** [Harvest](/blog/0-4-0-harvest)

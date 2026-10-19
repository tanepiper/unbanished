---
layout: home
title: Un-Banished
titleTemplate: Banished, rebuilt from scratch

hero:
  name: Un-Banished
  text: Banished, rebuilt from scratch
  tagline: A new engine for a classic city builder. It runs on Windows, macOS and Linux, and in your web browser. It uses the game files from your own copy of Banished.
  image:
    src: /assets/un-banished-icon.svg
    alt: The Un-Banished emblem, a timber house under a low sun
  actions:
    - theme: brand
      text: Play in your browser
      link: /play
    - theme: alt
      text: Setup guide
      link: /guide/
    - theme: alt
      text: How it was made
      link: /about/how-it-was-made

features:
  - title: The same game, exactly
    details: The same seed makes the same map. Citizens are born, work, age and die by the same rules, with the same numbers and the same timings as in Banished 1.0.7.
  - title: On every computer
    details: Native on Windows, macOS (Apple Silicon and Intel) and Linux. The browser version runs on WebGPU or WebGL 2.
  - title: Your towns still load
    details: Un-Banished reads and writes the original save format, in the original save folder. Your old towns appear in the Load dialog.
  - title: Mods work
    details: Mods made with the BanishedKit load through the Mods menu, with the same order and conflict rules as the original.
  - title: Nothing taken
    details: All code is new. No code, art, sound or text from the original is included. You bring your own copy of the game.
  - title: Free and open
    details: The source code is on GitHub under the MIT or Apache-2.0 licence. Every finding about the original is written down.
---

<div class="ub-wide">

<div class="ub-section">

## A town in the forest

<Gallery />

</div>

<div class="ub-divider"></div>

<div class="ub-section">

## Why this exists

Banished came out in February 2014. One person, Luke Hodorowicz of Shining Rock Software, made all
of it: the engine, the art, the sound and the design. It is a quiet game. You do not fight
anyone. You help a small group of exiles to survive. You plant fields, you cut wood, you build
houses, and you store food for the winter.

The game still has many players. But it runs only on Windows, and computers change. Un-Banished
is our thank-you to the game. We studied how every part of it works, and we built it again, so
that it can run on more computers for many more years.

We did not change the game, and we did not try to improve it. The goal is the same game, exactly.

<p class="ub-sign">If you love Banished, please buy it. Un-Banished needs your copy, and the person who made it deserves your support.</p>

[Read the full story](/about/) · [Buy Banished on Steam](https://store.steampowered.com/app/242920/Banished/)

</div>

<div class="ub-divider"></div>

<div class="ub-section">

## How it was made

We did not have the source code of Banished. We had only the finished program. Ghidra turned the
program's machine code into readable pseudocode. An AI agent read it, wrote down what each part
does, and ran the original functions in an emulator to get exact numbers. Then it wrote new code
in Rust from those notes, and tested the new code against the original.

<img src="/assets/process-loop.svg" alt="The seven steps of the work loop, from reading the data files to committing, with a path back to tracing when a check fails" style="width: 100%; max-width: 860px; margin: 24px 0;" />

[Read how each step works](/about/how-it-was-made)

</div>

<div class="ub-divider"></div>

<div class="ub-section">

## Start playing

There are two ways to play. In the browser, you only need the four game files. On the desktop, you
build Un-Banished once with Rust, which takes about ten minutes.

[Play in your browser](/play) · [Set it up on your computer](/guide/) · [See the status](/about/status)

</div>

</div>

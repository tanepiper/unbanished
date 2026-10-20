---
layout: home
title: Un-Banished
titleTemplate: Banished, rebuilt from scratch

hero:
  name: Un-Banished
  text: Banished, rebuilt from scratch
  tagline: A new engine for a classic city builder. It runs on Windows, macOS and Linux, and in your web browser. It uses the game files from your own copy of Banished.
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
    details: Native on Windows, macOS (Apple Silicon and Intel) and Linux. The browser version runs on WebGPU or WebGL 2, and installs as an app.
  - title: Your towns still load
    details: Un-Banished reads and writes the original save format, in the original save folder. Your old towns appear in the Load dialog.
  - title: Mods, and where to find them
    details: BanishedKit mods load as in the original. The Mods menu also browses and installs mods from Nexus Mods and banishedinfo.com, and a mod manager keeps mod lists.
    link: /guide/mods
    linkText: Mods
  - title: Play together
    details: Not in the original. Several players can build one town together, on the desktop and in the browser, through a small relay.
    link: /guide/multiplayer
    linkText: Multiplayer
  - title: Free, open, and all new
    details: All code is new, under the MIT or Apache-2.0 licence, with no code, art, sound or text from the original. A copy of Banished is required.
---

<script setup>
import { withBase } from 'vitepress'
</script>

<div class="ub-wide">

<div class="ub-section">

## Screenshots

<Gallery />

</div>

<div class="ub-divider"></div>

<div class="ub-section">

## In motion

Citizens at the market of a town of 116 people, and the same town from autumn to spring at the
highest speed. Un-Banished recorded both videos itself, without a window.

<div class="ub-videos">
  <video :src="withBase('/assets/videos/citizens-market.webm')" autoplay loop muted playsinline></video>
  <video :src="withBase('/assets/videos/seasons.webm')" autoplay loop muted playsinline></video>
</div>

[More pictures and videos](/guide/playing) · [How the videos are made](/develop/videos)

</div>

<div class="ub-divider"></div>

<div class="ub-section">

## About the project

Banished is a city-building game by Shining Rock Software, released in February 2014. It runs
only on Windows. Its last version is 1.0.7.

Un-Banished is a reimplementation of the game's engine in Rust. It reads the data files of an
installed copy of Banished and runs the game on Windows, macOS, Linux and in web browsers. The
goal is identical behaviour: the same maps, rules, numbers, timings and save files. Where
Un-Banished behaves differently from the original, this is treated as a bug.

Un-Banished contains no code, art, sound or text from the original game. A copy of Banished is
required, from [Steam](https://store.steampowered.com/app/242920/Banished/) or
[GOG](https://www.gog.com/en/game/banished).

[More about the project](/about/)

</div>

<div class="ub-divider"></div>

<div class="ub-section">

## How it was made

We did not have the source code of Banished. We had only the finished program. Ghidra turned the
program's machine code into readable pseudocode. An AI agent read it, wrote down what each part
does, and ran the original functions in an emulator to get exact numbers. Then it wrote new code
in Rust from those notes, and tested the new code against the original.

<WorkLoop />

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

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
  - title: No original content
    details: All code is new. No code, art, sound or text from the original is included. A copy of Banished is required.
  - title: Free and open
    details: The source code is on GitHub under the MIT or Apache-2.0 licence. Every finding about the original is written down.
---

<div class="ub-wide">

<div class="ub-section">

## Screenshots

<Gallery />

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

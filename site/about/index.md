---
title: About the project
description: What Banished and Un-Banished are, the project's goals, and what it includes.
---

# About the project

## Banished

Banished is a city-building game by Shining Rock Software, a one-person studio (Luke Hodorowicz).
It was released on 18 February 2014, for Windows only. The last version is 1.0.7.

In the game, the player directs a group of exiled travellers who build a town. The player does not
control the citizens directly. The player chooses buildings and jobs, and the citizens work, form
families, have children, grow old and die. The main challenges are food, firewood, clothing and
tools through the winters, and events such as fires, disease and tornadoes.

## Un-Banished

Un-Banished is a reimplementation of the Banished engine, written in Rust with wgpu. It reads the
data files (`.pkg`) of an installed copy of Banished and runs the game:

- natively on Windows, macOS (Apple Silicon and Intel) and Linux;
- in web browsers, through WebAssembly, with WebGPU or WebGL 2.

The source code of Banished is not available. The behaviour of the original was worked out from
the program files. [How it was made](/about/how-it-was-made) describes the method.

## Goals

- **Identical behaviour.** The same seed gives the same map. Rules, numbers, timings and the
  interface match version 1.0.7. A difference from the original is treated as a bug. The few
  intentional differences are listed in
  [DEVIATIONS.md](https://github.com/tanepiper/unbanished/blob/main/docs/DEVIATIONS.md).
- **Compatible files.** Saves from the original load in Un-Banished. Mods made
  with the BanishedKit (`.pkm`) work.
- **Documented findings.** Every finding about how the original works is written down in the
  [repository](https://github.com/tanepiper/unbanished/tree/main/docs).

Changes to the game's design or balance are not a goal of the project.

## What is included

| Included | Not included |
|---|---|
| New source code (Rust, WGSL shaders) | Code from the original game |
| Documents that describe the original's behaviour | Decompiled code or disassembly |
| A few screenshots of Un-Banished running | Art, sound, text or data files from the game |

A copy of Banished is required, from
[Steam](https://store.steampowered.com/app/242920/Banished/) or
[GOG](https://www.gog.com/en/game/banished).

## Licence

The source code is licensed under MIT or Apache-2.0, at your option. See
[Licence and legal](/about/licence).

Un-Banished is not affiliated with or endorsed by Shining Rock Software.

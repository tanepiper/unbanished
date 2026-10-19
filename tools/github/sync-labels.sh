#!/usr/bin/env bash
# Create or update the repository's issue labels (needs an authenticated `gh`).
# Labels not listed here are left alone; delete those by hand.
set -euo pipefail

label() { gh label create "$1" --color "$2" --description "$3" --force; }

# Type: what kind of issue. Each issue form sets one.
label "bug"           d73a4a "Something is broken, crashes or cannot be done"
label "parity"        b60205 "Works, but differs from Banished 1.0.7"
label "performance"   fbca04 "Frame rate, hitches, load time, memory"
label "enhancement"   a2eeef "Tooling, platform, docs or opt-in improvement"
label "documentation" 0075ca "Docs only"
label "research"      5319e7 "Reverse-engineering trace needed before a port"

# Area: which part of the game or repository.
label "area: selection"    c5def5 "Picking, highlight, clicking entities"
label "area: ui"           c5def5 "Building/entity windows, toolbar, menus, utility windows"
label "area: simulation"   c5def5 "Game iteration, scheduling, pause and speed"
label "area: jobs"         c5def5 "Professions, workplaces, citizens' work"
label "area: farming"      c5def5 "Crop fields, orchards, pastures"
label "area: construction" c5def5 "Placement, building sites, roads, removal"
label "area: rendering"    c5def5 "Renderer, shaders, camera, models"
label "area: audio"        c5def5 "Sound effects, music, ambience"
label "area: save-load"    c5def5 "Saves, autosave, loading"
label "area: formats"      c5def5 "crates/formats resource and save parsers"
label "area: tooling"      c5def5 "tools/, emulator, Ghidra scripts, CI"

# Platform: where it reproduces.
label "platform: web"     bfdadc "Browser (WASM) build"
label "platform: windows" bfdadc "Native Windows"
label "platform: macos"   bfdadc "Native macOS"
label "platform: linux"   bfdadc "Native Linux"

# Priority: set at triage.
label "priority: critical" b60205 "Crash, data loss or the game cannot be played"
label "priority: high"     d93f0b "Blocks a common action; no workaround"
label "priority: medium"   fbca04 "Has a workaround or affects a less common path"
label "priority: low"      0e8a16 "Cosmetic or rare"

# Status: where the issue stands.
label "status: needs triage" ededed "New; not yet labelled and prioritised"
label "status: needs repro"  ededed "Cannot reproduce yet; more detail wanted"
label "status: confirmed"    0e8a16 "Reproduced; cause or fix path known or being traced"
label "status: blocked"      000000 "Waiting on another issue or on missing evidence"

# Resolution and contributor labels (GitHub defaults, kept).
label "duplicate"        cfd3d7 "This issue or pull request already exists"
label "invalid"          e4e669 "Not a problem with Un-Banished"
label "wontfix"          ffffff "Out of scope, e.g. a gameplay change by default"
label "question"         d876e3 "Further information is requested"
label "good first issue" 7057ff "Good for newcomers"
label "help wanted"      008672 "Extra attention is needed"
label "accessibility"    f143ab "Barrier affecting people with disabilities"

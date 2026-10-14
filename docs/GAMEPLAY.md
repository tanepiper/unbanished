# Gameplay and data-flow findings

This file records small, evidence-backed slices of the game behavior as they are traced. It is a
working research log, not a claim that the simulation has been fully reconstructed. Decompiled output
is Ghidra pseudocode for the shipped Windows binary, not original source code or runtime observation.

## Evidence and repeatability

The first pass used Ghidra 12.1.4 headless with default auto-analysis on the Steam x64 Windows build.
The analyzed files were:

| File | SHA-256 |
|---|---|
| `Application-steam-x64.exe` | `323e0a12d1ef36605db48d44210745e7043240f07d2d0ae2cf7e13e3c727a5ed` |
| `Runtime-steam-x64.dll` | `d1c777b56674303ee276e807b88a7b73089d779f5dc923731ed45ef214cc7936` |

The project, exports and binary inputs are local under the git-ignored `analysis/` and `game-data/`
directories. The tracked scripts in `tools/ghidra/` export string references, decompile explicitly
selected addresses (including callback targets Ghidra did not identify as functions), and enumerate
selected 64-bit virtual-function tables. The prior run's detailed output is local evidence and is
intentionally not committed; this file preserves the findings and hashes needed to interpret it.

To repeat a focused export in an existing headless project:

```sh
ghidra_dir="$GHIDRA_INSTALL_DIR"
"$ghidra_dir/support/analyzeHeadless" analysis/gameplay-project banished-gameplay \
  -process Application-steam-x64.exe -noanalysis -scriptPath tools/ghidra \
  -postScript ExportGameplayRefs.java analysis/gameplay-references.txt
```

`ExportAddressBatch.java` takes an output path followed by one or more addresses. It will ask Ghidra
to disassemble and create a function at a supplied address when analysis has not already made one.
`ExportVtableSlots.java` takes output path, label, table address and optional slot count pairs. It
assumes 64-bit pointers and should only be used after identifying the table address from a relevant
constructor or registration record.
`ExportNamedFunctions.java` decompiles analyzed functions whose names contain any supplied search
terms; it is useful for tracing the separately analyzed Runtime DLL.

## Observed startup and new-game dialog path

The type-registration code registers `MapParamsDialog` as a `UI::DialogInstance` subtype and
`MapGenerator` as an `Object::External` subtype. Registration gives each class a factory and an
initializer callback; it does not establish when or how those objects are used during a game.

The main-menu action IDs and the New Game dialog's parameter handoff are now traced through the
loading-loop transition. See the dedicated [NEW-GAME-FLOW.md](NEW-GAME-FLOW.md) for the named
controls, selector initialization, observed caption/ID mismatch, and the remaining path into
`Game/Game` and map generation.

## Registered gameplay types

Static registrations establish these inheritance edges:

| Type | Registered base | What this establishes |
|---|---|---|
| `ResourceProduceComponent` | `ControllerComponent` | A controller-component family for resource-producing/consuming workplaces |
| `HuntComponent`, `ConsumeProduceComponent`, `HerbalistComponent`, `FishingComponent`, `ForesterComponent`, `MarketComponent` | `ResourceProduceComponent` | Concrete subclasses share that family |
| `CitizenAIComponent`, `MapComponent`, `NaturalResourceComponent`, `StorageComponent`, `WorkComponent` | `Component` | Separate component registrations exist for these simulation concerns |
| `CitizenDescription`, `WorkDescription`, `NaturalResourceDescription` | `Description` | These data descriptions are distinct from runtime components |

These are structural facts only. Registration does not prove update order, ownership, or the flow of
resource quantities between components. The class names make those useful next tracing targets, but
do not by themselves establish their gameplay rules.

## Description serialization observed in virtual tables

For the selected description tables, slot 5 points to a serializer override and a later slot points
to the constructor initializer. The `WorkDescription` serializer at `0x140133d80` serializes, in
order, integer fields at object offsets `+0x24`, `+0x28`, `+0x2c`, a boolean at `+0x34`, an integer at
`+0x30`, and a boolean at `+0x20`. Field names are absent from these serializer calls; the meanings
are not assigned here.

`CitizenDescription`'s serializer at `0x140119dd0` serializes a larger set of numeric and vector-like
fields, including a three-element array and an integer-driven helper at `+0x138`; it also has no
field names in the stream. `MapGenerator`'s serializer at `0x1400a7e50` includes a base record,
strings, a boolean, integer and float values, counted arrays, and filename-like strings. The full
field sequence is preserved in the local Ghidra export for follow-up. These traces show data shape,
not what the fields mean or how generation consumes them.

## Open paths to trace

The overall rebuild objective remains much larger than this first slice. The next findings should
follow data across the boundaries below, recording each boundary in its own dedicated document or
an appropriately focused section here:

1. New-game control IDs → named button/resource bindings → map-parameter dialog.
2. Continue the partially traced map-settings and seed path through terrain samples, water, trees,
   rocks and initial resource placement; see [WORLD-GENERATION.md](WORLD-GENERATION.md) and its
   detailed [terrain-grid transform trace](TERRAIN-GRID-TRANSFORMS.md). The Hard/Medium/Easy
   starting-condition inputs are documented in [START-CONDITION-PROFILES.md](START-CONDITION-PROFILES.md).
3. Description/template load → entity construction → component ownership and lifecycle/update order.
   Component allocation, initialization, update dispatch and deferred removal are detailed in
   [ENTITY-COMPONENT-LIFECYCLE.md](ENTITY-COMPONENT-LIFECYCLE.md).
4. Work selection → pathfinding and task execution → production/consumption → storage and market transfer.
   See [RESOURCE-PRODUCTION-FLOW.md](RESOURCE-PRODUCTION-FLOW.md) for the current partial trace of
   recipes, inventory ledgers, reservations and market restocking.
5. Citizen needs, household assignment, birth/death, health, education and migration transitions.
   Current static findings are in [CITIZEN-NEEDS-AND-HOUSEHOLDS.md](CITIZEN-NEEDS-AND-HOUSEHOLDS.md).
6. Calendar, seasons, weather, crop/forest growth and natural-resource regeneration/depletion.
7. Construction, upgrades, demolition, disasters, trading, taxation and events.
8. Save/load serialization and restoration order, including any state rebuilt rather than serialized.

The first Runtime DLL save-container and restore-path trace is in
[SAVE-FORMAT-AND-RESTORE.md](SAVE-FORMAT-AND-RESTORE.md). Save headers, both compression layers and
metadata decode; the gameplay-state object tree remains open.

Until these paths are traced against code and shipped data, the repository's simulation milestones
remain incomplete. Runtime verification against the game is a separate future step; static findings
must not be described as observed runtime behavior.

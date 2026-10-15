# Jobs, professions and construction (investigation)

Status: **research in progress.** No job, profession-assignment or construction behaviour is
ported yet, which is why a placed building gets no builders and citizens cannot be assigned to
work (the ProfessionUI spinners and the WorkPlaceUI buttons only display values). This note
collects the evidence gathered so far and the porting plan. Evidence: Ghidra pseudocode of the
Steam x64 executable (`tools/triage/decomp.py`, `types.py`).

## Classes involved

| Class | Parent | Notes |
|---|---|---|
| `Work` (`0x845c4242`) | `Object::Internal` | base of every job; vtable slot `+0x20` tries to give the work to a citizen and returns 0 (no), 1 (not now) or 2 (taken); `+0x48` refreshes it |
| `BuildStructure` (`0xddc6f030`, ctor `0x1400eaf60`) | `Work` | builders' construction work; `+0x20` `FUN_1400ea490` (2009 bytes), `+0x48` `FUN_1400ea0b0` |
| `ClearArea` (`0x33859be8`, ctor `0x1400ebda0`) | `Work` | clearing trees/rocks under a footprint; `+0x20` `FUN_1400eb3b0`, `+0x48` `FUN_1400eafe0` |
| `Pickup` (`0x25d5c240`, ctor `0x1400f90d0`) | `Work` | hauling raw materials; `+0x20` `FUN_1400f83d0` (2537 bytes) |
| `BuildRoad`, `GatherRawMaterials`, `PlantArea`, `PlantOrchard`, `TendArea`, `FishingWork`, `FireWork`, `MarketWork`, `TradeWork`, `LocationWork`, `ConsumeProduce`, `ClearStorage`, `AutoPickupWork` (+ `ForesterWork`, `HerbalistWork`, `HuntWork`, `PastureWork`) | `Work` | the other professions' work |
| `WorkComponent` / `WorkDescription` | `Component` / `Description` | per-entity work registration (`Template\X.rsc:work`) |
| `WorkPlaceComponent` / `WorkPlaceDescription` | `ControllerComponent` / `Description` | workers of a building (WorkPlaceUI) |
| `BuildComponent` (`0x5a9b4240`, ctor `0x14010e490`) / `BuildDescription` | `ControllerComponent` | construction site; main vtable `0x140168680`, controller interface `0x140168630` |
| `Profession`, `ProfessionList` | `Object::External` | `Game/Profession/*.rsc` |

## Job manager (`Game +0x118`)

The AI's actions 2 (emergency work) and 9 (work) call into it:

- `FUN_1401107a0` → `FUN_1400fcd00` (`Jobs::AssignWork`); `FUN_140110810` → `FUN_1400fca20`
  (`Jobs::AssignEmergency`). Both report success when the citizen's action stack
  (`+0x278 → +0x60`) is non-empty afterwards.
- **AssignWork** (`FUN_1400fcd00`), when the manager has work (`+0x38 != 0`):
  1. if the citizen's profession (`+0x1f0`) is not the laborer profession (profession
     manager `Game +0xb0 → +0x30`), look the profession up in the tree at `+0x120` and try its
     work list (`FUN_1400fce80`);
  2. otherwise, or when that fails, try emergency work (`FUN_1400fca20`);
  3. then the laborer list.
- **AssignEmergency** (`FUN_1400fca20`): a citizen with an emergency (`FUN_14011b0b0`) takes
  category 14 only. Otherwise the categories come from the stock manager (`Game +0xf0 → +0x170`
  slots, count vs `limit / 10`): food low → 1; firewood → 2, 4, 10; … (eight stock checks,
  each listing category ids terminated by -1); builders also get category 13 first. One list
  is chosen with `Rand(n)` on the game stream, then its categories are tried in order, each a
  work tree at `Jobs + (cat + 3)·0x10`.
- **FUN_1400fce80**: uses the citizen's workplace (`+0x248`) when set (and not an
  emergency); collects candidate works ordered by distance (`FUN_1400fd160`: path rank
  `FUN_1400a4ce0` from the citizen's cell to the work's cell `+0x28`, else map size + straight
  distance; ties by `+0x20`) or by priority (`FUN_1400fd4d0`: `+0x24`, ties by `+0x20`), then
  `FUN_1400fd030` offers each in order: works that failed this pass (`+0x2c == pass`) are
  skipped; `+0x48` refreshes, `+0x20` tries; 2 = taken, 1 = remember the failure.

Work fields seen so far: `+0x10` owning entity/workplace, `+0x20` id (tie-break), `+0x24`
priority, `+0x28` map cell, `+0x2c` last failed pass, `+0x4c` open to any profession.

## BuildComponent

- Serialize (`FUN_14010e250`): `+0x48` messages received, `+0x4c` stage, `+0x50` flag, then the
  ModelComponents (`+0x58`) and PickingComponents (`+0x68`) of the construction meshes.
- Activate (`FUN_14010e210`): unless the game is loading (`Game +0xd0 & 1`), collects its own
  and any MultiObjectComponent children's model and picking components, resets their
  animation/visibility and sets the stage count `+0x4c` (1, or the multi-object count).
- Message (`FUN_14010dbe0`): message 0 increments `+0x48`.
- The controller interface (`0x140168630`) dispatches state callbacks from a static table at
  `DAT_1401dbca0` (records of `0xf8` bytes: enter/exit/update callbacks, transitions keyed by
  message id), so the construction steps live in those state callbacks together with the
  building's `StateMachineDescription` (`Template\X.rsc:statemachine`).
- `BuildDescription` (`FUN_14010e610`): requirements `(count, raw material)`, work amount
  `+0x30`, multi flag `+0x34` (already parsed for the CreateBar).

## Plan

1. Decode the StateMachine table (`DAT_1401dbca0`) and the building `statemachine`
   descriptions: the states a placed building passes through (clear area, gather materials,
   build, complete) and the messages between them.
2. Port the job manager's data structures and `FUN_1400fce80`/`FUN_1400fd030` with the
   existing AI dispatcher (actions 2 and 9), verified with emulator cases like the candidate
   sort.
3. Port `ClearArea`, `Pickup` (deliveries to a site) and `BuildStructure` work, so laborers
   clear and haul and builders build; construction stages swap the models.
4. Port profession assignment (ProfessionUI spinners → laborers become builders, etc.) and the
   WorkPlaceUI add/remove buttons, as commands from the UI into the simulation.
5. Then the other professions' work, one at a time.

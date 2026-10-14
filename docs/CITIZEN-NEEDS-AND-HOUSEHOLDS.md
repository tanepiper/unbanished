# Citizen needs, households, and simulation updates

## Scope and evidence

The Windows gameplay executable contains separate `CitizenComponent` and
`CitizenAIComponent` classes. This trace follows their update dispatch, the hunger,
cold, health and happiness fields, and residence/family membership. It establishes
static behavior and field relationships, not a measured reproduction of a running
settlement.

Evidence comes from Ghidra 12.1.4, with no Hopper analysis:

- Existing export: `analysis/export/Application-steam-x64.exe/decomp.c` and
  `functions.tsv`.
- Fresh, read-only headless decompilation and incoming references from
  `analysis/gameplay-project/banished-gameplay`, program
  `Application-steam-x64.exe`, using `-readOnly -noanalysis`.
- Fresh focused outputs: `/tmp/citizen-update-trace.txt` and
  `/tmp/citizen-household-trace.txt`; these are local working evidence, not retained
  repository artifacts. The function addresses below remain usable with the full
  ignored export or original executable.
- Original executable SHA-256:
  `323e0a12d1ef36605db48d44210745e7043240f07d2d0ae2cf7e13e3c727a5ed`.
- Existing Ghidra C export SHA-256:
  `46a8a175d932e0347961a421b052946b773a9da92e1d12bccd556dd6b4cca1c3`.

Application methods lack original symbols. `FUN_140...` names below are Ghidra
names; descriptive labels in this report are analyst labels. Component identities
are stronger observations because the executable registers their literal class
names and constructor functions. No tests or target execution were performed for
this report.

## Component identity and the actual update path

`0x140154610` registers `CitizenComponent` with factory `0x140030a70`. That factory
allocates `0x290` bytes and calls constructor `0x140119c40`, which installs vtable
`0x1401689a0`. `0x140154590` separately registers `CitizenAIComponent`, whose factory
`0x140030480` allocates `0xd0` bytes and calls `0x140115040`, installing vtable
`0x140168890`.

| Class / vtable | Slot | Target | Observed purpose |
| --- | --- | --- | --- |
| Citizen / `0x1401689a0` | `+0x20` | `0x140119520` | Resolve Character, status, and CitizenDescription references; initialize health/happiness |
| Citizen / `0x1401689a0` | `+0x48` | `0x140118340` | Advance needs, age, disease, household relationships and reproduction |
| Citizen / `0x1401689a0` | `+0x78` | `0x140117730` | Receive numbered component messages, including warming and eating |
| Citizen / `0x1401689a0` | `+0x18` | `0x140119630` | Serialize citizen state and typed internal references, block version 3 |
| CitizenAI / `0x140168890` | `+0x20` | `0x140114df0` | Cache the entity's CitizenComponent at AI `+0x38` |
| CitizenAI / `0x140168890` | `+0x48` | `0x1401148b0` | Advance action timers and reconsider hunger/cold and other work |
| CitizenAI / `0x140168890` | `+0x18` | `0x140114e30` | Serialize AI state, block version 5, including 16 cooldown timers |

The component update is reached from the game loop, rather than from UI drawing:

1. Game-loop update `0x14009b850` first checks whether the UI pauses the game. For
   each requested simulation iteration, it calls entity manager `0x1400c9560`.
2. `0x1400c9560` advances manager time and visits scheduled entities. For an entity
   due for updating, the delta is manager time minus that entity's timestamp at
   `+0x20`; the delta can therefore cover multiple manager iterations.
3. An entity must have aggregate flag `0x8` and must not have flag `0x400` set.
   The manager scans its component array (`+0x30`, count `+0x38`), checks component
   flag `0x8` at component `+0x20`, and calls **vtable slot `+0x48` with that delta**.
   It stops scanning if the entity acquires `0x400` during the update.
4. Both Citizen and CitizenAI constructors enable that update flag. Their two
   methods therefore run through the same dispatch mechanism.

This establishes the dispatch and time-delta calculation. It does not establish
which of those two components appears first in every template's component array,
or the exact per-category scheduling quotas of the entity manager.

## Citizen state and needs

Offsets in this section are relative to `CitizenComponent`; `D` denotes its cached
`CitizenDescription` at `+0x288`.

| Offset | Interpretation | Evidence / confidence |
| --- | --- | --- |
| `+0x28` | Citizen name string | Initialized by `0x14011c550`; read by CitizenUI `0x140065440` |
| `+0x38` | Sex selector | Initialization and male/female UI; pairing requires unequal values |
| `+0x3c` / `+0x40` | Age / individual death-age threshold | Advanced and compared by `0x140118340`; age UI; death threshold randomized in `0x14011c550` |
| `+0x44` | Hunger accumulator | No-hunger debug flag, food handling and death path independently agree |
| `+0x48` | Cold accumulator | No-cold debug flag, temperature gate and warming callback agree |
| `+0x4c` | Reproduction / recovery countdown | Birth creation and cooldown branches in `0x140118340` |
| `+0x50` / `+0x54` / `+0x58` | Health / accumulated health events / periodic timer | CitizenUI maps `+0x50` to `groupHearts`; update calls `0x140117e60` |
| `+0x68` / `+0x70` / `+0x78` | Disease description / disease duration / spread timer | Disease selection, transmission and recovery in `0x140117ca0`, `0x140117940`, `0x140117880` |
| `+0x80` / `+0x88` / `+0x90` | Happiness / accumulated events / periodic timer | CitizenUI maps `+0x80` to `groupStars`; update calls `0x140117fb0` |
| `+0x98` / `+0xa0` | Distinct food-resource array / length | Eating records previously unseen resource descriptions in `0x14011a7b0` |
| `+0x1e0` / `+0x1e4` / `+0x1e8` | Clothing state / effective insulation state / expiry timer | `textClothing` UI, messages 6–7, and cold-rate indexing; exact garment names remain data-driven |

The heart/star interpretation is supported by both `CitizenUI` initialization
`0x140065b00` (`groupHearts` stored at UI `+0xa0`, `groupStars` at UI `+0xc0`) and
its refresh method `0x140065440`, which supplies citizen `+0x50` with maximum
`D+0x70`, and citizen `+0x80` with maximum `D+0xa4`. Initialization `0x140119520`
sets health from `D+0x6c` and happiness from `D+0xa4`.

### Accumulation, thresholds and death

Let `dt` be `System::Time::AsSeconds(delta)`. Fresh Ghidra data reads establish
`0x140178934 = 300.0` and `0x14017895c = 3600.0`. `0x140118340` applies:

- Age increases by `dt * D[0x40]`.
- Unless global no-hunger bit 1 is set, hunger increases by
  `dt * D[0xc0] / 3600`. The debug UI explicitly names that bit `noHungerCheck`.
  If disabled by the debug flag, hunger is set to zero. A sick citizen attending
  the service at `+0x228` has both hunger and cold cleared before accumulation.
- Cold is cleared when global no-cold bit 2 is set, or when the current climate
  value at game-loop `+0x130`, then `+0x18`, reaches `D[0xf4]`. Otherwise cold
  increases by `dt * D[0xdc + 4 * effective_insulation] / 300`.
- Hunger is capped at `D[0xd0]`; cold is capped at `D[0xf0]`. These bounds can lead
  to `0x14011c2a0` with reason 0 or 1 respectively, subject to the Character action
  state gate visible at the call sites. Age exceeding `+0x40` uses reason 2.
- `0x14011c2a0` emits the corresponding notification and queues deletion of the
  citizen's entity. Global debug bit 8 makes that helper return without deleting.

The default constructor `0x14011a340` sets `D+0xc0=100`, `D+0xcc=75`,
`D+0xd0=100`, `D+0xec=75`, `D+0xf0=100`, `D+0xf4=48`; these are **constructor
fallbacks**, not a claim that every compiled/modded CitizenDescription uses them.
The climate value's display unit and its time-of-day interpolation were not
traced here.

### AI actions that reduce needs

CitizenAI update `0x1401148b0` decreases its sixteen cooldowns and checks hunger
against `D+0xcc` and cold against `D+0xec`. It can request a fresh action selection
with `0x140113060`. Candidate builder `0x140113170` adds urgent hunger/cold actions
14/15 with priority field 7; normal variants use action codes 0/1. Candidate order
is randomized and then sorted before `0x140113060` tries candidates until an
active Character action exists. The complete comparator/tie-breaking semantics
are not established by this trace.

Dispatcher `0x1401146c0` maps 0/14 to `0x140112250` (food destination selection),
and 1/15 to `0x140112790` (warm destination selection). The former considers home
food and other reachable food sources; the latter considers a warm destination
and a warm home. They construct action sequences via `0x1400c28f0` and
`0x1400c2c60`, including movement work and component-directed messages. The full
movement/task scheduler is outside this trace, but the receivers are identified:

| Citizen message | Receiver behavior |
| --- | --- |
| 0 | `0x1401176b0`: resolve the supplied entity's ResidenceComponent; clear cold only if `0x140130100` says it is warm |
| 2 | `0x14011a7b0`: choose food, remove one inventory unit through `0x140132620`, subtract **1** hunger, repeat until satisfied or food selection fails; clamp hunger to zero |
| 3 | Set health-event bit `0x200` and add `0.5` health; the semantic service type is not established here |
| 6 / 7 | Set clothing and effective insulation to 1 / 2; start a timer of `D[0x130] * 300` |

The eat helper also records food diversity and copies category bits from
`RawMaterialDescription+0x5c` into health-event bits. Its caller can consume home
food directly during AI update when food is available, so needs reduction is not
limited to a separately completed movement sequence.

Residence warm predicate `0x140130100` returns true when climate value is at least
`ResidenceDescription+0x28`, or the ResidenceComponent timer at `+0x48` is positive.
Residence update `0x14012ee40` can consume a stored resource with raw-material
category bit 6 and refresh that timer. Its stock getter `0x14012ffd0` sums that
category. Calling it heating fuel is a strong inference from these consumers;
the exact resource names and amounts come from the resource data.

### Health and happiness are periodic state changes

`0x140118340` accumulates independent timers and repeatedly consumes elapsed
intervals before calling the two helpers:

| Update | Interval in simulation seconds | Helper |
| --- | --- | --- |
| Health | `D[0x78] * 300` | `0x140117e60` |
| Happiness | `D[0xa0] * 300` | `0x140117fb0` |

The health helper counts four category-event bits (`0x10`, `0x20`, `0x40`, `0x100`)
and service bit `0x200`. With `ate` bit `0x8`, additional bit `0x4`, and more than
three counted events it adds `0.5`; with `ate`, no `0x200`, and fewer than three
counted events it subtracts `1 - count/3`. Hunger/cold event bits 1/2 each subtract
1. An adult without the additional bit `0x4` and without the attendance at
`+0x208` loses `0.25`. The result is clamped to `[0, D[0x70]]`, then health-event
bits are cleared. Global no-health debug bit 3 instead sets maximum health and
returns. Bit `0x4` is deliberately left unnamed: its producers include issuing
need actions, and its complete meaning is not yet established.

The happiness helper applies recorded negative/positive events, builds a target
from other event bits, food variety, residence-area contributions, and attendance
at `+0x230`, then approaches the target by `0.5` per call and clamps to
`[0, D[0xa4]]`. Its target scale is `0.125 * D[0xa4]`. Residence-area contribution
counts five nonzero map-cell values, subtracts a sixth, clamps below zero, and is
quartered for residences with multiple family slots. Individual event bits and
those map-cell categories should remain numeric until their producers are
traced. The body does not unconditionally clear happiness-event bits at its end.

## Home assignment and household relationships

Citizen home ownership is an entity reference plus a family-slot index; it is not
just a position or a boolean. Serializer `0x140119630` type-checks the references
below against the registered `ComponentEntity` / `CitizenComponent` classes.

| Citizen offset | Relationship | Evidence |
| --- | --- | --- |
| `+0x238` / `+0x240` | Home entity / family slot within its ResidenceComponent | Home button `0x140065270`; setter `0x14011b8a0`; Residence occupant arrays |
| `+0x248` | Workplace entity | Profession navigation and work assignment; distinct from home |
| `+0x250` | Partner citizen | Opposite-sex search, reciprocal pointer assignment, shared home and birth conditions |
| `+0x258` | Parent/guardian citizen | Set when inserting a new child/dependent; used to inherit home |
| `+0x260` | Dependent-ring head | First child/dependent of this citizen |
| `+0x268` / `+0x270` | Next / previous dependent in a circular ring | Insert/remove helpers maintain both directions |

`ResidenceComponent` is explicitly registered through type-info record
`0x1401cfcf0`. Its family-slot array is at residence `+0x50`, with slot count
at `+0x58`. Each `0x10`-byte slot holds a vector of citizen references and its
count. Occupancy helper `0x14012f4c0` sums the counts; `0x14012f9c0` compares one
slot's count against `ResidenceDescription+0x24` to decide whether that household
is full. Multiple family slots therefore differ from multiple occupants in a
single family slot.

### Assigning or moving a citizen

Setter `0x14011b8a0(citizen, home, slot)` is the central mutation:

1. If home or slot changes, remove the citizen from the old ResidenceComponent
   via `0x14012fa20`.
2. Store the new entity reference and family-slot index.
3. If the home is non-null, append the citizen to its family-slot vector through
   `0x14012fbe0`; that helper sorts occupants and starts occupied-building effects
   when appropriate.
4. Traverse the citizen's dependent ring and recursively assign each dependent
   the same home and slot.

A child's home is thus inherited through a relationship pointer and an explicit
setter call, not selected independently on each update. `0x14011bd70` inserts a
dependent into the circular ring, sets its guardian pointer, then assigns the
parent's current home and slot. `0x140117470` changes the guardian relationship;
`0x140117510` removes the old ring links and clears the removed dependent's home.

### Finding a home, pairing, and births

Home search `0x140116340` first tries a reachable partner's home when its
ResidenceComponent has one family slot. Otherwise it scans registered residence
entities, resolves Residence and placement components, and considers empty family
slots. It checks map-cell type masks and connectivity-region identifiers, prefers
a free single-family residence, and retains a shared-residence fallback when the
caller permits it. This is an ordered scan, not an observed nearest-distance
minimization. Existing residents' upgrade/move checks also inspect other adult
citizens; the precise fairness policy should remain unresolved because several
helper signatures are incompletely recovered.

In `0x140118340`, an underage citizen or attendee at `+0x208` can inherit their
guardian's home if they lack one. Eligible adults without a home mark a happiness
event, search for a home, detach from their guardian when moving, and assign both
themselves and an existing partner to the chosen slot. Citizens in shared
residences may subsequently search for a single-family residence.

Partner search `0x140115310` scans registered citizens and returns the first
candidate with a different sex selector, no existing partner, age strictly above
`D+0x48`, and age difference at most `D+0x4c`. The update stores reciprocal partner
pointers and puts the partner in the same home/slot. No relationship-kinship test
is visible in this particular helper; that bounded observation does not prove
none exists elsewhere.

The birth path in `0x140118340` requires sex selector 0, adulthood, no attendance
at `+0x208`, a partner, a home, the configured age window (`D+0x50..D+0x54`), and a
household not reported full by `0x14012f9c0`. Hunger/cold can cancel a positive
pregnancy countdown. When it expires, a health-dependent random comparison either
calls child creator `0x140117060` or the death helper with reason 6. Child creation
uses the parent's entity template, initializes the new citizen at the parent's
position with age zero through `0x14011c550`, inserts it into the entity manager,
and emits a birth notification. The exact linkage arguments in the old full
export are partially lost to Ghidra's inferred calling signatures; the dependent
insert/remove mechanics above are independently explicit.

## Coverage and remaining reconstruction work

Answered statically: component identities, simulation dispatch slot and delta,
hunger/cold accumulation and relief callbacks, health/happiness periodic update
helpers, home/family-slot storage, dependent propagation, pairing prerequisites,
and the birth path's main guards.

Still incomplete:

- All AI priority/comparator rules, movement/task execution order, and which
  component runs first in each concrete citizen entity.
- Original source field names, the full happiness-event bit dictionary, food
  category names, and the residence-area map-cell categories.
- Every home-upgrade/fairness branch, complete pregnancy/guardian argument
  recovery, and cross-checking these branches against a live settlement.
- Exact shipped CitizenDescription values rather than constructor defaults,
  full resource selection/path costs, disease probabilities, and climate units.

A port needs both serialized scalar state and the shared entity-reference graph.
Persisting home references without Residence occupant vectors, guardian rings,
partner pointers, AI timers and need-event accumulators would discard behavior
that this update path explicitly reads and mutates.

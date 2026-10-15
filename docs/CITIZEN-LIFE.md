# Citizen life

Ports: `crates/game/src/life.rs` (CitizenComponent update, ResidenceComponent),
`crates/game/src/needs.rs` (eating, warming, stocking the home), `crates/game/src/agents.rs`
(order, births, removal). The research these build on is in
[CITIZEN-NEEDS-AND-HOUSEHOLDS.md](CITIZEN-NEEDS-AND-HOUSEHOLDS.md).

## Order

Per simulation iteration, for each citizen: the CitizenComponent update (`FUN_140118340`), then
the CitizenAIComponent's (the template's component order). A citizen queued for deletion skips
its AI (entity flag `0x400`). When an action sequence ends, the AI's message handler
(`FUN_140114810`) first runs the CitizenComponent update with zero time, then the AI. The
character is idle at that moment, and that is when hunger, cold and old age can kill
(`stack_top + 1 < 1`). Deaths are processed after every citizen has updated
(`FUN_140119170`, the removal slot).

## CitizenComponent update (`FUN_140118340`)

`D` is the CitizenDescription; `dt` is the elapsed time.

| Step | Rule |
|---|---|
| Age | `+= dt · D[0x40]` (1/3600: a year is 3600 s) |
| Coming of age | crossing `D[0x44]` posts event 9 (or 10 with a school place) |
| Hunger | `+= dt · D[0xc0] / 3600`; capped at `D[0xd0]`; at the cap an idle citizen starves (0) |
| Cold | 0 when the temperature is at least `D[0xf4]`, else `+= dt · D[0xdc + 4·clothing] / 300`; capped at `D[0xf0]`, where an idle citizen freezes (1) |
| Old age | dies (2) once age reaches the lifespan `+0x40` |
| Disease | `FUN_140117ca0` (below); death reason 3 |
| Status icons | 0 homeless, 1 hunger above `(D[0xd0] − D[0xcc])/2 + D[0xcc]`, 2 cold above `(D[0xf0] − D[0xec])/4 + D[0xec]`, 5 sick; above those lines health events 1/2 and flag 1 |
| Health | every `D[0x78]·300` s, `FUN_140117e60` |
| Happiness | every `D[0xa0]·300` s, `FUN_140117fb0` |
| Home | an unreachable home is given up; a child follows its guardian's; a homeless adult (or one still with a guardian) searches (`FUN_140116340`) and moves with its partner; a family in a shared residence looks for its own |
| Marriage | an adult with a home, no guardian and no partner, at least `D[0x48]` old, takes the first registered single of the other sex older than `D[0x48]` within `D[0x4c]` years (`FUN_140115310`); both get flag `0x200` |
| Pregnancy | women only: hunger or cold above the warning lines cancels it; at the end a random draw against `health/D[0x70] · (D[0x64] − D[0x68]) + D[0x68]` gives a child (`FUN_140117060`) and a wait of `RandFloat · D[0x60] + D[0x5c]`, or the mother dies (6); a married woman with a home, aged `D[0x50]..D[0x54]`, whose family is not full starts one of `D[0x58]` seconds |

Births: the child's sex evens out the unmarried no older than `D[0x4c]` (a boy when the boys'
share is at most `RandFloat`, else `Rand(2)`), then `FUN_14011c550` draws its name,
appearance and lifespan. It joins its mother's ring and home and the registry, and the log
gets event 8. The parents get flag `0x100`.

Deaths (`FUN_14011c2a0`): the family is flagged (`0x2` for old age, `0x80` otherwise, `0x40`
when buried), the log gets the reason, the entity is queued. Removal: the character's actions
are abandoned (works fail), the citizen leaves school, doctor, chapel, home and guardian; a
widow(er) takes the dependents, else children die (reason 4) and older dependents leave; the
partner is single again; the workplace is left, and a laborer takes over the profession
(`Town::Hire`, event 0xf, or 0x10 when nobody can).

### Health (`FUN_140117e60`)

Count event bits `0x10, 0x20, 0x40, 0x100` (food kinds) and `0x200` (treated). With "ate"
(`8`) and bit `4` and more than three: +0.5. Ate, untreated, fewer than three: −(1 − n/3).
Hunger (1) and cold (2) events: −1 each. An adult without bit 4: −0.25. Clamp to
`0..D[0x70]`; clear the events.

### Happiness (`FUN_140117fb0`)

Flags: `2` −8, `0x40` −2, `0x80` −4, `8` −1, `0x10` −1; `0x100..0x2000` +1 each; `0x40000`
+0.5. Clamp. The target is `0.125 · D[0xa4]` times: +1 each without `4`, `0x20`, `1`;
`0x20000` +0.5 or +1; food variety (`0x10000`) −1 at most `D[0xa8]` kinds, +1 from `D[0xac]`;
the home's surroundings; +1 with a chapel. Happiness moves 0.5 toward it.

The flags at `+0x88` are serialized, and a scan of the whole executable finds nothing that
clears them after the constructor. The port keeps them set. This needs checking against a
real save.

### Disease (`FUN_140117ca0`)

Healthy: immunity counts down. The risk mixes `D[0x7c..0x90]` by weakness
(`1 − health/D[0x70]`) and the town's visitor risk (`+0x1a8`). Every update draws `RandFloat`,
and the citizen falls ill when it is below `elapsed / (a · 12 · 300 · b)`. Only one outbreak
runs at a time (`FUN_14008e890`). The disease is picked by weight from
`Game/Disease/Diseases.rsc` (`FUN_140117af0`), with event 0xb at severity 3. Illness lasts
`(RandFloatTol · D[0x98] + D[0x94]) · 300` s. Every `spread_interval · 300` s a
`spread_chance` draw may infect the first healthy citizen within `spread_radius`. At the end
the citizen dies when `RandFloat ≤ 1/recovery/(D[0x128] with a doctor)`, else recovers, immune
for `D[0x9c] · 300` s.

`D[0x94]`, `D[0x98]` and `D[0x9c]` are not stored in the resource; the constructor's
values are 1, 0.5 and 12.

## Residences

`ResidenceDescription`: families, family size, warm temperature, seconds per firewood. A
finished residence registers (`FUN_14012f2b0`) and gets empty family slots. Its update
(`FUN_14012ee40`) burns down the fire. While it is colder than the house's warmth, one unit of
firewood relights it for `fuel_seconds`. Food and firewood clear the status icons 6 and 7.

## Activities

| Activity | Port |
|---|---|
| Eat (0, urgent 14) | home food (urgent: eaten on the spot), else the town's storages (`FUN_140093200`); urgent: also empty residences (`FUN_140093840`). Walk, 2 s of work, Citizen message 2 (eat until not hungry, one unit per point), plus message 0 at a warm home. Failure: 3.75 s cooldown on both slots |
| Warm (1, urgent 15) | urgent: the nearest warm residence (`FUN_140094140`); else a warm home. Walk, 4 s, message 0. Failure: 3.75 s cooldown |
| Stock home food / firewood (6 / 7) | drop off first if carrying; pick up from the nearest storage as much as the larger of the laborer's carry and the free capacity, carry it home, warm up there. No source: cooldown `D[0xd8]·300` s and the home's icon 6/7 |

The AI's inputs: home food is `families × members × D[0xc4]` (`FUN_14011abb0`), home firewood
`families × D[0xd4]` (`FUN_14011acc0`), and the partner's errand stops both going.

## Services (`crates/game/src/services.rs`)

| Piece | Original | Port |
|---|---|---|
| AttendanceComponent (school, chapel, doctor) | state 2 enter `FUN_1400445f0`: register at town `+0x60`, a LocationWork, enable the workplace; exit `FUN_140044590` (icon 2 when every attendance of the kind is full); state 3 `FUN_140044530`: unregister, everyone leaves, disable; scheduled update `FUN_140044cd0`: everyone leaves when nobody works there | yes (the icon is computed, building icons are not drawn) |
| LocationWork (`0x140167ab0`) | one staff member at a time walks to the nearest use point and works 1 s, then completes (and repeats) | yes |
| HealerComponent | state 2 registers at town `+0x70`, state 3 unregisters | yes |
| Joining (`FUN_140116e00` + `FUN_140044a50`) | the attendance whose workplace's profession matches `D+0x20/+0x28/+0x30`, with room, a worker and reachable, nearest the home (path rank between entrances) | yes |
| School | at coming of age a place (event 10) or adulthood (9); message 4 adds a lesson; at `D[0x118]` lessons or after `D[0x11c]` hours: educated, out of school, event 9 | yes |
| Chapel | searched every update while without one; +1 to the happiness target | yes |
| Doctor | searched when falling ill and while ill; message 5 shortens the illness by `D[0x12c]` minutes; recovering leaves | yes |

Activities: school (10) sits a lesson of `D[0x120]` s at the school's nearest use point, or
leaves and looks for another school (`FUN_14011af20(c, 1)`). Doctor (5) is treated for
`D[0x124]` s (reporting "not started", like the original), or rests at home 5 s without a
doctor. Herbs (4) take a herb from the nearest storage to the nearest staffed herbalist and are
treated there for 5 s (message 3). Play (11) walks next to the guardian's partner (the
original's `Rand(1)` is always 0), or the guardian, and idles `RandFloat · 7 + 3` s.

## Happiness buildings (`crates/game/src/happiness.rs`)

HappinessComponent is a controller. Its table is `FUN_14004b570`, and its interface vtable
is `0x14015f6a0`.

| Piece | Original | Port |
|---|---|---|
| HappinessDescription (`FUN_14004b610`) | `+0x20` kind; `+0x24` needs workers; `+0x25` needs stock; `+0x28` raw-material mask; `+0x2c` loiter padding. The kinds are 0 well, 1 market and trading post, 2 tavern, 3 chapel and cemetery, 4 doctor, 5 mines and quarry | yes |
| Active (`FUN_14004aeb0`) | Kind 5 always counts. With workers needed, the workplace must be staffed and switched on. With stock needed, the storage must hold a raw material matching the mask | yes |
| States 2 and 0xd | Enter (`FUN_14004b350`) registers an active building. The scheduled update (`FUN_14004b300`) registers or unregisters it as its activity changes | yes |
| State 3, removal | `FUN_14004b2a0`, `FUN_14004b380`: unregister | yes |
| Register (`FUN_14004ae50` / `FUN_14004ae00`) | Join or leave the town manager's list (`+0x50`), and add ±1 on the kind's map layer (kinds above 4 use layer 5) within the radius around the map rectangle's centre (`FUN_14004ac60` → `FUN_1400a4e60`) | yes |
| Visit (message 0, `FUN_14004b0e0`) | With stock needed, one unit of the first matching, unreserved raw material is used up, and the stock slot's consumption counter goes up | yes; the counter is not ported |
| Loiter target (`FUN_14004b080`, `FUN_14008eea0`) | A registered building that is not kind 5. The pick is `Rand(count)` over those whose first use point is within path rank `2 · range` and reachable | yes |

Happiness (`FUN_140117fb0`) reads the home entrance cell's layers. Layers 0–4 count one
each when nonzero, layer 5 subtracts its value, and the total is at least 0; a shared home
gets a quarter of it. The entrance is `FUN_14008d480`, which draws `Rand(n)` for homes with
several use points.

The citizens' entity views are taken once per update, so a building registered during an
iteration becomes a loitering target in the next one.

## Verification

`placement_tool::construction_tests`:
- `a_family_moves_in_stocks_the_house_and_has_a_child` (seed 1234, Medium): a family of five
  moves into the new wood house, eats from storage, carries 100 food home, and a child is born
  into the household.
- `citizens_starve_without_food`: with the food removed, all 15 starve at the end of their
  sequences, and the registry and families let go of them.
- `a_teacher_staffs_the_school_and_a_student_learns`: a new schoolhouse gets a teacher; a child
  coming of age takes a place there and sits a lesson.
- `a_well_registers_its_happiness`: a new well registers, marks layer 0 within its radius
  and becomes a loitering target.

## Not ported yet

The workplace searches
(`FUN_140115490`, `FUN_140115940`, `FUN_140115eb0`), burial and cemeteries, accidents (`+0x1f8`), the residence's ClearStorage work, and the stock
consumption counters.

# Achievements

`Game\Achievement\AchievementList.rsc` (an ExternalList, loaded by `FUN_14009f280` and given to
`AchievementManager::SetAchievements`) lists 36 achievements. Each is an
`Achievement::Achievement` (Runtime `Serialize` `0x18009eb70`): `+0x20` its string table,
`+0x28` title and `+0x30` description keys, `+0x38` sprite sheet and `+0x40` sprite, `+0x48` its
goals (`Achievement::Goal`, `0x18009fd30`: a string table and text, then the class's block).

Port: `apps/game/src/achievements.rs` (checking), `achievement_ui.rs` (the dialog),
`crates/formats/src/scenario.rs` (the formats).

## Checking (`AchievementManager::Update`, `0x18009f1e0`)

After each frame's iterations the game calls `Update(dt × iterations)` (`FUN_14009b850`). Nothing
happens, and no time is gathered, while user pack files (mods) are enabled (`FileSystem +0x38`,
`AreUserPackFilesEnabled`). Every
achievement not yet awarded gathers the time; the next one not awarded (`+0xb0`, in turn) has
its goals updated with it (`GoalList::Update`), and with every goal met and none failed it is
awarded (`SetAchievementMet`) and the game logs event `0x19` with its title (`FUN_14009be30`).

A goal's state (`GoalInstance::CheckGoalState` → `Goal` slot `+0x40`) is 0 not met, 1 met,
2 failed; `GoalInstance::Update` keeps it in `+0x10` (bit 0 met, bit 1 failed) and a failed goal
stays failed.

- `BuildingGoal` (`FUN_1400dca50`): `+0x30` all of the buildings `+0x38` exist, else any.
- `PopulationGoal` (`FUN_1400dccf0`): the population is at least `+0x30`.
- `TimeGoal` (instance `FUN_1400e0100` / `FUN_1400e0180`): its time grows while the other
  goals are met and goes back to none when they are not; met at `+0x30` years (× 3600 s).
- `FinishTutorialGoal`: the tutorial is done (see [TUTORIAL.md](TUTORIAL.md)).
- `StatGoal` (`FUN_1400dfbf0`): statistic `+0x30` (with the reference `+0x48`) against `+0x3c`,
  or for odd comparisons against statistic `+0x34` (reference `+0x60`); comparisons `+0x38`
  0/1 equal, 2/3 not, 4/5 less, 6/7 at most, 8/9 more, 10/11 at least. With `+0x40` the goal
  is met until the comparison holds, then fails (Isolationist: no trading post). `+0x78` are
  raw-material flags and `+0x7c` a number for the statistics that take them.

### Statistics (`FUN_1400df9b0`)

| # | Function | Value |
|---|---|---|
| 0 | `FUN_1400df8e0` | educated working adults (grown, not at school, education ≥ CitizenDescription `+0x118`) |
| 1 | `FUN_1400df770` | working adults whose tool (`+0xa8`) is the material's (`+0x54`) or past use (< 0) |
| 2 | `FUN_1400df620` | citizens in a coat (`+0x1e0` 2 or 4 for a warm material, flag `0x8000`; 1 or 4 otherwise) |
| 3, 4 | `FUN_1400df570`, `FUN_1400df4c0` | the mean happiness (`+0x80`), health (`+0x50`) |
| 5 | stock manager `+0xa8` | the population |
| 6, 7, 8 | `FUN_1400df3f0`, `FUN_1400df350`, `FUN_1400df290` | working adults, students, children |
| 9 | `FUN_1400df1a0` | working adults of the profession |
| 10, 11 | `FUN_1400df150`, `FUN_1400df0e0` | the game's terrain, climate is the reference |
| 12 | `FUN_1400df050` | graves with a tombstone |
| 13 | stock manager `+0x1ac` | the value traded away |
| 14 | `FUN_1400deb20` | what storages, homes and trading posts (of the template) hold of the flags |
| 15, 16, 17 | `FUN_1400dea30`, `FUN_1400de940`, `FUN_1400de850` | known livestock, crops, trees (SelectDescription `+0x40` 2, 0, 1) |
| 18 | `FUN_1400de7a0` | the animals in pastures of the kind |
| 19 | `FUN_1400de240` | the kinds of the flags this year's workplaces (of the template) made (TrackerComponent) |
| 20 | `FUN_1400de120` | the stock slot's production over the last `+0x7c` years (the statistics' made series) |
| 21 | `FUN_1400dddb0` | what this year's workplaces of the template made of the flags |
| 22 | `FUN_1400ddbb0` | the workers at the template's workplaces |
| 23 | `FUN_1400ddb70` | the map's built road cells of kind `+0x7c` (`Game +0x110 → +0x220`) |
| 24 | `FUN_1400ddac0` | residences (of the template) |
| 25 | `FUN_1400dd680` | the template's buildings standing (built, not torn down or being removed) |
| 26 | `Game +0x128` | the year |
| 27 | `Game +0x34` | the map size |
| 28 | stock manager `+0x1b0` | nomads taken in |
| 29 | `FUN_1400dd1c0` | the longest side of the template's standing buildings |

## The dialog (`AchievementDialog`, main menu id 8)

Bind `FUN_14005ddf0`: `tableAchievement` gets a `groupAchievement` per achievement (`imageIcon`
its sprite, `textTitle`, `textDescription`, `imageStar` hidden); refresh `FUN_14005dba0`: the
star shown and the icon (`0xffe0e0e0`, else `0xff303030`) and texts (`0xffc0c0c0`, else
`0xffb0b0b0`) bright once awarded, `textComplete` the share awarded; `buttonOk` (100) closes it.

Deviations: what is awarded is kept in the settings store (`Achievement<title>`) instead of
Steam's user stats; statistic 23 counts the cells each time (the original keeps the count when
the map's road meshes are rebuilt); the game's terrain is compared by its name's resource
(`Game\Terrain\<name>.rsc`).

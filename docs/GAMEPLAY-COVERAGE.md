# Gameplay coverage audit and plan

Audit date: 2026-10-15. Goal: every map and gameplay feature of the original working before
save/load (M7), because a save must round-trip every component's state.

The audit enumerated the original's registered classes (`tools/triage/types.py list`), the
shipped templates and the component descriptions each uses (`game-data/extracted/Template`),
and the port's sources and docs. A class counts as ported only when its behaviour runs in the
simulation. Parsing it or drawing its mesh is not enough.

## Components

| Component | Templates | Port |
|---|---|---|
| Map, Model, Picking, Highlight, UI, Interact, Zone | all buildings | yes |
| NaturalResource, Growth, Spawn (same template), RawMaterial | resources, piles | yes (cross-template spawn open) |
| Character, CitizenAI | Citizen | movement, actions, AI choice; activities 2, 3, 8, 9, 12 |
| Storage, StorageLocation | storage buildings, citizens | ledgers, search, reservations |
| Build, ClearArea, WorkPlace, StateMachine, Work | buildings | construction, professions |
| ResourceProduce (ConsumeProduce) | CutterYard, Tailor, ToolMaker, Tavern | cutter yard verified; the rest share the code |
| StatusIcon | citizens, buildings | yes ([STATUS-ICONS.md](STATUS-ICONS.md); BuildDestroy with demolition) |
| AmbientEmitter, AmbientEnvironment | most buildings | environment, wind, river, rain |
| Citizen (`0x1401689a0`) | Citizen | needs, age, death, health, happiness, disease, homes, marriage, births, burial, accidents, ClearStorage ([CITIZEN-LIFE.md](CITIZEN-LIFE.md)); consumption counters open |
| Residence | WoodHouse, StoneHouse, Hostel | families, heating, moving in, ClearStorage, moving out (state 3) |
| **Immigration**, **Nomad** | TownHall, Nomad | yes ([IMMIGRATION.md](IMMIGRATION.md); bands, nomads, taking in, visitor risk; the town hall window and ImmigrationUI) |
| Attendance | School, Chapel, Doctor | yes (staff by LocationWork, places, lessons, treatment) |
| **Happiness** | Tavern, Well, Market, Cemetery, Chapel, TradingPost, Quarry, Mine | yes (`happiness.rs`; map layers, registry, visits, loitering pick) |
| Healer, Disease (citizens) | Herbalist | registry and herb treatment |
| Disease (blight) | fields, orchards, pastures | yes ([FARMING.md](FARMING.md#blight-cratesgamesrcblightrs)) |
| **Cemetery**, **Tombstone** | Cemetery | yes ([CITIZEN-LIFE.md](CITIZEN-LIFE.md#burial-cratesgamesrccemeteryrs)) |
| **Well**, **Fire** | Well; every building | yes ([FIRE.md](FIRE.md); ignition, spread, buckets, wells, emergencies, rubble) |
| **Market**, **MarketLocation** | Market | yes ([MARKETS.md](MARKETS.md)) |
| **Trade**, **Boat** | TradingPost, TraderBoat | yes ([TRADE.md](TRADE.md); merchants, boat, offers, automatic trading, TradeWork, the trade window and exchange) |
| **Destroy** | every building | yes ([JOBS.md](JOBS.md#demolition-cratesgamesrcdestroyrs); demolition, refunds, cancel, damage, rubble) |
| **CropField** | fields | yes ([FARMING.md](FARMING.md)) |
| **Orchard**, **Fruit** | orchards | yes ([FARMING.md](FARMING.md)) |
| **Pasture**, **Livestock** | pastures, animals | yes ([FARMING.md](FARMING.md); breeding, culling, products, starting livestock) |
| **Herd**, **Population**, **WildAnimal**, **Animal** | deer, livestock | yes (`wildlife.rs`) |
| ResourceProduce subtype **Herbalist** (gather) | gatherer's shelter, herbalist | yes (`herbalist.rs`) |
| ResourceProduce subtype **Forester** | forester's lodge | yes (`forester.rs`; toggle buttons not wired) |
| ResourceProduce subtype **Fishing** | fisherman's dock | yes (`fishing.rs`, map layer 8) |
| ResourceProduce subtype **Hunt** | hunter's lodge | yes (`hunt.rs`, HuntAnimal, RemoveAnimal) |
| **AutoPickup** | gatherers, hunters, foresters | yes |
| **AbsorbResource**, **StorageDisplay**, **StorageLocation** | StorageYard, barns | yes ([JOBS.md](JOBS.md#storage-sites-storagelocation-absorbresource-storagedisplay)) |
| **MultiObject**, **Terraform**, **Floor** | bridges, tunnels, fences, dock | yes (fenced parts, bridge and tunnel pieces; [BRIDGES.md](BRIDGES.md), [TUNNELS.md](TUNNELS.md)) |
| **Decal**, **Particle** | roads, footprints, smoke | missing |
| **Tracker**, **Value** | workplaces, livestock | Tracker and the stock's made/used counters and history ([TOWN-INFO.md](TOWN-INFO.md)); Value in trade |
| **Tornado** | Tornado | yes ([TORNADO.md](TORNADO.md); spawn, path, wrecking; its particles not drawn) |

## Works and actions

| Work | Port | | Action | Port |
|---|---|---|---|---|
| BuildStructure, GatherRawMaterials, ClearArea, Pickup, ConsumeProduce, LocationWork, FishingWork, gatherer, forester and hunter works | yes | | MoveTo, PlayAnimation, PerformWork, SendComponentMessage, CompleteTask, DropOffRawMaterial, PickupRawMaterial, PickupStoredRawMaterial, ShowTool, AnimateNaturalResource, RemoveNaturalResource, CreateRawMaterial | yes |
| BuildRoad | yes ([ROADS.md](ROADS.md)) | | CompleteRoad | yes |
| PlantArea, TendArea, harvest (fields) | yes | | ConsumeRawMaterial (`FUN_1400cb880`) | herbs (from what is carried); eating and warming use Citizen messages |
| PlantOrchard, fruit picking | yes | | CreateNaturalResource (planting) | yes |
| | | | HuntAnimal, RemoveAnimal | yes |
| AutoPickupWork (base of the gatherer, forester, hunter, harvest and pasture works), PastureWork, ClearStorage | yes | | | |
| MarketWork, FireWork, TradeWork | yes | | | |

## Citizen activities (`FUN_1401146c0`)

| Id | Activity | Port |
|---:|---|---|
| 0, 14 | eat (`0x140112250`) | yes |
| 1, 15 | warm up (`0x140112790`) | yes |
| 2 | emergency work | yes |
| 3, 8 | clothing, tool | yes |
| 4, 5 | herbs, doctor | yes |
| 6, 7 | bring food / firewood home | yes |
| 9 | work | yes |
| 10 | school | yes |
| 11 | play (children) | yes |
| 12 | idle / loiter | yes |
| 13 | drop off what is carried | partly (no redirect) |

## Tools, UI and map features

| Feature | Port |
|---|---|
| Building placement, removal of resources, ClearCancel | yes |
| Roads (dirt, stone), Remove Roads | yes ([ROADS.md](ROADS.md)); road drawing (decals) no |
| Bridges (BridgeTool, floors) | yes ([BRIDGES.md](BRIDGES.md)) |
| Tunnels (terraform) | yes ([TUNNELS.md](TUNNELS.md)) |
| Fields, orchards, storage yard (DragPlacementTool) | yes (`removal_tool.rs` area mode: check `FUN_140100fb0`, apply `FUN_140100dd0`, overlay `FUN_140101310`) |
| Pastures, cemetery (FencedTool) | yes (`place_fenced`, the drag in `removal_tool.rs`, the shelter turned by the rotation keys) |
| Demolition (Remove Buildings, DestroyUI) | yes (Remove Buildings and its cancel, DestroyUI's buttons) |
| Building windows ([BUILDING-WINDOWS.md](BUILDING-WINDOWS.md)) | yes |
| Town Info, Production/Population graphs | yes ([TOWN-INFO.md](TOWN-INFO.md), TownHallUI) |
| Achievements, tutorial, scenarios | yes ([ACHIEVEMENTS.md](ACHIEVEMENTS.md), [TUTORIAL.md](TUTORIAL.md); the tutorials start new games until saves load) |
| Weather precipitation, smoke particles, decals, fire particles | no |
| Disasters (fire, tornado, disease) and AutoSlow | yes (fire, blight and tornadoes with the Disasters option; AutoSlow from their events) |

## Plan

Each stage ends with tests (original-function cases where the existing emulator tools reach them,
otherwise seeded integration runs), the determinism check, the WASM build, and a commit.

1. **Citizen life.** CitizenComponent update `0x140118340`, messages `0x140117730`, death
   `0x14011c2a0`, health `0x140117e60`, happiness `0x140117fb0`. ResidenceComponent families,
   heating `0x14012ee40`, home search `0x140116340`, partner search `0x140115310`, births
   `0x140117060`. Activities eat, warm, home food and firewood, drop off. Tombstone and cemetery.
2. **Food and resource production.** ResourceProduce subtypes: gatherer, hunter (with deer
   herds), fisherman, forester (planting), herbalist, quarry, mine. AutoPickup, AbsorbResource.
3. **Farming.** CropField (PlantArea, TendArea, seasons), Orchard, Pasture and livestock,
   crop-selection UI.
4. **Services and town life.** Attendance (school, chapel, doctor), happiness sources (tavern,
   well, market), market vendors, healer and disease, wells and fire, immigration and nomads,
   trading post with the trader boat and its UI.
5. **Infrastructure.** Roads and road speed, bridges, tunnels, fences, demolition, decals and
   floors, MultiObject.
6. **Events and presentation.** Tornado, disasters, AutoSlow, particles (smoke, weather),
   weather lighting, Town Info and trackers, achievements, tutorial.

Save/load (M7) follows stage 6.

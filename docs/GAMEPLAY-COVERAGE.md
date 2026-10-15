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
| StatusIcon | citizens, buildings | citizen bits; building bits missing |
| AmbientEmitter, AmbientEnvironment | most buildings | environment, wind, river, rain |
| Citizen (`0x1401689a0`) | Citizen | needs, age, death, health, happiness, disease, homes, marriage, births ([CITIZEN-LIFE.md](CITIZEN-LIFE.md)); attendance searches, burial and accidents open |
| Residence | WoodHouse, StoneHouse, Hostel | families, heating, moving in; ClearStorage work open |
| **Immigration**, **Nomad** | TownHall, Nomad | missing |
| Attendance | School, Chapel, Doctor | yes (staff by LocationWork, places, lessons, treatment) |
| **Happiness** | Tavern, Well, Market, Cemetery, Chapel, TradingPost, Quarry, Mine | yes (`happiness.rs`; map layers, registry, visits, loitering pick) |
| Healer, Disease (citizens) | Herbalist | registry and herb treatment; crop disease (fields) missing |
| **Cemetery**, **Tombstone** | Cemetery | missing |
| **Well**, **Fire** | Well; every building | missing |
| **Market**, **MarketLocation** | Market | missing |
| **Trade**, **Boat** | TradingPost, TraderBoat | missing |
| **Destroy** | every building | missing (demolition) |
| **CropField**, **Orchard**, **Pasture**, **Livestock**, **Fruit** | fields, orchards, pastures, animals | missing |
| **Herd**, **Population**, **WildAnimal**, **Animal** | deer | yes (`wildlife.rs`; livestock breeding with pastures) |
| ResourceProduce subtype **Herbalist** (gather) | gatherer's shelter, herbalist | yes (`herbalist.rs`) |
| ResourceProduce subtype **Forester** | forester's lodge | yes (`forester.rs`; toggle buttons not wired) |
| ResourceProduce subtype **Fishing** | fisherman's dock | yes (`fishing.rs`, map layer 8) |
| ResourceProduce subtype **Hunt** | hunter's lodge | yes (`hunt.rs`, HuntAnimal, RemoveAnimal) |
| **AutoPickup** | gatherers, hunters, foresters | yes |
| **AbsorbResource**, **StorageDisplay** | fields, StorageYard | missing |
| **MultiObject**, **Terraform**, **Floor** | bridges, tunnels, fences, dock | missing |
| **Decal**, **Particle** | roads, footprints, smoke | missing |
| **Tracker**, **Value** | workplaces, livestock | missing (Town Info graphs, production counters) |
| **Tornado** | Tornado | missing |

## Works and actions

| Work | Port | | Action | Port |
|---|---|---|---|---|
| BuildStructure, GatherRawMaterials, ClearArea, Pickup, ConsumeProduce, LocationWork | yes | | MoveTo, PlayAnimation, PerformWork, SendComponentMessage, CompleteTask, DropOffRawMaterial, PickupRawMaterial, PickupStoredRawMaterial, ShowTool, AnimateNaturalResource, RemoveNaturalResource, CreateRawMaterial | yes |
| BuildRoad | no | | CompleteRoad | no |
| PlantArea, TendArea (fields) | no | | ConsumeRawMaterial (`FUN_1400cb880`) | herbs (from what is carried); eating and warming use Citizen messages |
| PlantOrchard | no | | CreateNaturalResource (planting) | no |
| FishingWork | no | | HuntAnimal, RemoveAnimal | no |
| AutoPickupWork, ClearStorage, MarketWork, TradeWork, FireWork | no | | | |

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
| Roads (dirt, stone), Remove Roads | no |
| Bridges, tunnels (drag placement, terraform) | no |
| Fields, orchards, pastures, storage yard (drag areas) | placement only (CreateDrag parsed) |
| Fences, cemetery (CreateFenced) | no |
| Demolition (Remove Buildings, DestroyUI) | no |
| Residence, upgrade, market, trade, field crop selection windows | no |
| Town Info, Production/Population graphs | no |
| Achievements, tutorial, scenarios | no |
| Weather precipitation, smoke particles, decals, fire | no |
| Disasters (fire, tornado, disease) and AutoSlow | no |

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

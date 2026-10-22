//! Families, food, school, deaths and burials, nomads, and the merchant's boat.

use super::*;

/// A finished wood house gets a family, who eat from the town's storage, carry food home
/// and have a child there.
#[test]
fn a_family_moves_in_stocks_the_house_and_has_a_child() {
    with_town(|archive, terrain, town, rng| {
        let mut tool =
            PlacementTool::start(archive, town, r"Template\WoodHouse.rsc:resource", rng).unwrap();
        let spot = tool.find_clear_spot(town, terrain, town.start).unwrap();
        tool.hover(town, terrain, Some(spot), 0);
        let id = tool.place(archive, town, 0).unwrap();
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let mut built = false;
        for _ in 0..30 * 60 * 8 {
            step(town, terrain, rng, 50.0);
            if town.construction.sites[&id].complete {
                built = true;
                break;
            }
        }
        assert!(built, "the house was not built");
        // Hungry enough to eat (`D[0xc8]`), not yet urgently.
        let hungry = town.agents.life.f(0xc8) + 1.0;
        for s in &mut town.agents.states {
            s.hunger = hungry;
        }
        let mut moved_in = None;
        for k in 0..30 * 60 * 3 {
            step(town, terrain, rng, 50.0);
            if moved_in.is_none() && town.construction.homes[&id].occupants() > 0 {
                moved_in = Some(k);
            }
            let home = &town.construction.homes[&id];
            if moved_in.is_some()
                && unbanished_game::people::life::stored(
                    &town.storages[home.storage],
                    &town.materials,
                    5,
                ) > 0
                && town.agents.states.iter().any(|s| !s.foods.is_empty())
            {
                break;
            }
        }
        let home = town.construction.homes[&id].clone();
        eprintln!("moved in after {moved_in:?}: {:?}", home.families);
        assert!(moved_in.is_some(), "nobody moved in");
        let family = &home.families[0];
        for &c in family {
            assert_eq!(town.agents.states[c].home, Some(id));
        }
        let food =
            unbanished_game::people::life::stored(&town.storages[home.storage], &town.materials, 5);
        eprintln!("food at home: {food}");
        assert!(food > 0, "nobody carried food home");
        assert!(
            town.agents.states.iter().any(|s| !s.foods.is_empty()),
            "nobody ate"
        );
        assert!(town.citizens.iter().all(|c| c.alive), "someone died");
        // A married woman of the family: her pregnancy ends at once.
        let mother = family
            .iter()
            .copied()
            .find(|&c| {
                let ci = &town.citizens[c];
                !ci.male && ci.spouse.is_some() && ci.age >= town.rules.adult_age
            })
            .expect("no married woman in the house");
        let before = town.citizens.len();
        town.agents.states[mother].reproduction = 0.001;
        town.agents.states[mother].health = 10.0;
        for _ in 0..10 {
            step(town, terrain, rng, 50.0);
        }
        assert_eq!(town.citizens.len(), before + 1, "no child was born");
        let child = before;
        assert_eq!(town.citizens[child].parent, Some(mother));
        assert_eq!(town.agents.states[child].home, Some(id));
        assert!(town.construction.homes[&id].families[0].contains(&child));
        assert!(town.agents.registry.contains(&child));
        assert!(
            town.agents.states[mother].reproduction < 0.0,
            "no wait after the birth"
        );
    });
}

/// Without food, hungry citizens starve once idle: the entity goes, the family and the
/// registry let go of it, and the log hears of it.
#[test]
fn citizens_starve_without_food() {
    with_town(|_, terrain, town, rng| {
        for s in &mut town.storages {
            let materials = &town.materials;
            s.items
                .retain(|e| materials[e.material].flags >> 5 & 1 == 0);
        }
        let max = town.agents.life.f(0xd0);
        for s in &mut town.agents.states {
            s.hunger = max - 0.01;
        }
        let n = town.citizens.len();
        let mut events = Vec::new();
        for _ in 0..30 * 30 {
            step(town, terrain, rng, 50.0);
            events.extend(std::mem::take(&mut town.agents.events));
            if town.citizens.iter().all(|c| !c.alive) {
                break;
            }
        }
        let dead = town.citizens.iter().filter(|c| !c.alive).count();
        eprintln!("{dead} of {n} starved");
        assert!(dead > 0, "nobody starved");
        assert_eq!(town.agents.registry.len(), n - dead);
        assert!(events
            .iter()
            .any(|e| e.kind == unbanished_game::people::life::reason::STARVED));
        for (i, c) in town.citizens.iter().enumerate().filter(|(_, c)| !c.alive) {
            assert!(c.spouse.is_none());
            assert!(!town.agents.characters[i].stack.is_active());
        }
    });
}

/// A staffed school takes a child coming of age, who sits lessons there (message 4).
#[test]
fn a_teacher_staffs_the_school_and_a_student_learns() {
    with_town(|archive, terrain, town, rng| {
        let mut tool =
            PlacementTool::start(archive, town, r"Template\SchoolHouse.rsc:resource", rng).unwrap();
        let spot = tool.find_clear_spot(town, terrain, town.start).unwrap();
        tool.hover(town, terrain, Some(spot), 0);
        let id = tool.place(archive, town, 0).unwrap();
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::SetCount {
                profession: builder,
                count: 3,
            });
        let mut built = false;
        for _ in 0..30 * 60 * 12 {
            step(town, terrain, rng, 50.0);
            if town.construction.sites[&id].complete {
                built = true;
                break;
            }
        }
        assert!(built, "the school was not built");
        assert!(town.construction.attendance_list.contains(&id));
        let teacher = town.agents.life.services[0].expect("no teacher profession");
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::AddWorker { entity: id });
        for _ in 0..60 {
            step(town, terrain, rng, 50.0);
        }
        let staff = town.construction.workforce.workplaces[&id].workers.clone();
        assert_eq!(staff.len(), 1, "no teacher hired");
        assert_eq!(town.agents.states[staff[0]].profession, teacher);
        // A child about to come of age.
        let adult = town.rules.adult_age;
        let child = (0..town.citizens.len())
            .find(|&c| town.citizens[c].age < adult)
            .expect("no child");
        town.citizens[child].age = adult - 0.0001;
        let mut learned = false;
        for _ in 0..30 * 60 * 4 {
            step(town, terrain, rng, 50.0);
            if town.citizens[child].education > 0 {
                learned = true;
                break;
            }
        }
        eprintln!(
            "school {:?}, attendees {:?}, education {}",
            town.agents.states[child].school,
            town.construction.attendance[&id].attendees,
            town.citizens[child].education
        );
        assert_eq!(town.agents.states[child].school, Some(id));
        assert!(learned, "no lesson");
    });
}

/// A fenced cemetery is built and lays out its graves; a citizen who dies of old age is
/// buried there (a tombstone on a grave) and the spouse grieves less (bit `0x40`).
#[test]
fn the_dead_are_buried_in_the_cemetery() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            return;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    let world = crate::view::world::World::generate(
        &archive,
        &crate::view::world::WorldParams {
            seed: 1234,
            ..Default::default()
        },
    )
    .unwrap();
    let terrain = HeightGrid {
        width: world.terrain.width(),
        heights: &world.terrain.heights,
    };
    let mut rng = Random::new(1234);
    let mut town = StartingTown::create(
        &archive,
        "Game/StartCondition/Easy.rsc:resource",
        world.terrain.size,
        &terrain,
        &mut rng,
    )
    .unwrap();
    let n = town.cells.size as i32;
    let size = 8;
    let empty = |town: &StartingTown, x: i32, y: i32| {
        (y..y + size).all(|cy| {
            (x..x + size).all(|cx| {
                let i = (cy * n + cx) as usize;
                cx < n
                    && cy < n
                    && town.cells.types[i] == unbanished_game::terrain::CellType::GROUND
                    && town.cells.objects[i].is_none()
                    && town.cells.occupants[i].is_none()
                    && town.cells.flags[i] == 0
            })
        })
    };
    let (sx, sy) = town.start;
    let rect = (8..80)
        .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
        .map(|(dx, dy)| (sx + dx, sy + dy))
        .find(|&(x, y)| x > 0 && y > 0 && empty(&town, x, y))
        .expect("no empty ground");
    let id = town
        .place_fenced(
            &archive,
            r"Template\Cemetery.rsc:resource",
            rect,
            (rect.0 + size - 1, rect.1 + size - 1),
            0,
            &terrain,
        )
        .expect("the cemetery was not placed");
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 4,
        });
    let mut warnings = unbanished_game::events::StockWarnings {
        limits: [100_000; 21],
        ..Default::default()
    };
    let dt = 1.0 / 30.0;
    let (mut built_at, mut buried_at) = (None, None);
    let mut widow = None;
    for step in 0..30 * 60 * 30 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 60.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 60.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        if built_at.is_none() && town.construction.cemeteries.contains(&id) {
            built_at = Some(step);
            // Old age comes for a married citizen.
            let c = (0..town.citizens.len())
                .find(|&c| town.citizens[c].alive && town.citizens[c].spouse.is_some())
                .expect("no married citizen");
            widow = town.citizens[c].spouse;
            let age = town.citizens[c].age;
            town.citizens[c].lifespan = age;
        }
        if built_at.is_some() && !town.construction.tombstones.is_empty() {
            buried_at = Some(step);
            break;
        }
    }
    let graves = town.construction.sites[&id].graves.clone();
    eprintln!(
        "built {built_at:?}, buried {buried_at:?}, {} graves, tombstones {:?}",
        graves.len(),
        town.construction.tombstones
    );
    assert!(built_at.is_some(), "the cemetery was not built");
    assert!(!graves.is_empty(), "no graves laid out");
    assert!(buried_at.is_some(), "nobody was buried");
    let (&stone, _) = town.construction.tombstones.iter().next().unwrap();
    let p = &town.placed[stone];
    let cell = p.cell.1 * n + p.cell.0;
    assert!(graves.contains(&cell), "the tombstone is not on a grave");
    assert_eq!(town.cells.objects[cell as usize], Some(stone));
    let w = widow.unwrap();
    assert_ne!(
        town.agents.states[w].flags & 0x40,
        0,
        "the spouse knows of the burial"
    );
}

/// With accidents made near certain for builders, a builder's work step fails with an
/// injury and the builder dies of it (code 7, the profession's death message).
#[test]
fn a_builder_dies_in_an_accident() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            return;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    let world = crate::view::world::World::generate(
        &archive,
        &crate::view::world::WorldParams {
            seed: 1234,
            ..Default::default()
        },
    )
    .unwrap();
    let terrain = HeightGrid {
        width: world.terrain.width(),
        heights: &world.terrain.heights,
    };
    let mut rng = Random::new(1234);
    let mut town = StartingTown::create(
        &archive,
        "Game/StartCondition/Medium.rsc:resource",
        world.terrain.size,
        &terrain,
        &mut rng,
    )
    .unwrap();
    let builder = town.construction.professions.builder;
    let death = town.construction.professions.list[builder].death.clone();
    assert!(!death.is_empty(), "the builder has no death message");
    assert!(0.0 < town.construction.professions.list[builder].accident.1);
    town.construction.professions.list[builder].accident = (1e-6, 1.0);
    let mut tool = PlacementTool::start(
        &archive,
        &mut town,
        r"Template\WoodHouse.rsc:resource",
        &mut rng,
    )
    .unwrap();
    let spot = tool
        .find_clear_spot(&town, &terrain, town.start)
        .expect("no spot");
    tool.hover(&town, &terrain, Some(spot), 0);
    tool.place(&archive, &mut town, 0).unwrap();
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 2,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let mut events = Vec::new();
    for _ in 0..30 * 60 * 8 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        events.extend(std::mem::take(&mut town.agents.events));
        if events
            .iter()
            .any(|e| e.kind == unbanished_game::people::life::reason::ACCIDENT)
        {
            break;
        }
    }
    let e = events
        .iter()
        .find(|e| e.kind == unbanished_game::people::life::reason::ACCIDENT)
        .expect("no accident");
    assert_eq!(
        e.extra.as_ref().map(|(_, k)| k.as_str()),
        Some(death.as_str())
    );
    assert_eq!(
        town.agents.states[e.citizen].accident,
        Some(builder),
        "the accident's profession"
    );
}

/// A town hall calls a band of nomads in spring; they walk in from a map edge and wait, and
/// taking them in makes them citizens (couples with their children) with the visitors'
/// disease risk.
#[test]
fn nomads_are_taken_in() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            return;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    let world = crate::view::world::World::generate(
        &archive,
        &crate::view::world::WorldParams {
            seed: 1234,
            ..Default::default()
        },
    )
    .unwrap();
    let terrain = HeightGrid {
        width: world.terrain.width(),
        heights: &world.terrain.heights,
    };
    let mut rng = Random::new(1234);
    let mut town = StartingTown::create(
        &archive,
        "Game/StartCondition/Medium.rsc:resource",
        world.terrain.size,
        &terrain,
        &mut rng,
    )
    .unwrap();
    let mut tool = PlacementTool::start(
        &archive,
        &mut town,
        r"Template\TownHall.rsc:resource",
        &mut rng,
    )
    .unwrap();
    let spot = tool
        .find_clear_spot(&town, &terrain, town.start)
        .expect("no spot");
    tool.hover(&town, &terrain, Some(spot), 0);
    let id = tool.place(&archive, &mut town, 0).unwrap();
    // The market and trading post the town would need are not placed here.
    let st = town.construction.sites[&id].template;
    let d = town.construction.templates[st]
        .immigration
        .as_mut()
        .expect("no immigration");
    assert_eq!(d.required.len(), 2);
    d.required.clear();
    d.population = 1;
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 4,
        });
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let before = town.citizens.iter().filter(|c| c.alive).count();
    let (mut called, mut band, mut asked, mut admitted) = (false, 0, false, false);
    let mut supplied = false;
    let mut states = Vec::new();
    for _ in 0..30 * 60 * 40 {
        // Spring (month 1).
        town.construction.calendar = 3600.0 / 12.0 * 1.5;
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        let site = town.construction.sites.get_mut(&id).unwrap();
        let state = site.machine.state;
        if states.last() != Some(&state) {
            states.push(state);
        }
        if state == 1 && !supplied {
            // The starting stock is short of the hall's stone: it is delivered.
            supplied = true;
            let storage = site.storage;
            for &(n, m) in &town.construction.templates[st].materials {
                town.storages[storage].add(m, n);
            }
        }
        if state == 0xf && !called {
            // The years between bands pass.
            called = true;
            site.immigration.as_mut().unwrap().timer = 0;
        }
        if state == 0x10 {
            band = band.max(site.immigration.as_ref().unwrap().nomads.len());
        }
        if state == 0x11 && !asked {
            asked = true;
            // The waiting nomads wear the NomadDescription's materials.
            let worn: Vec<String> = crate::view::town::TownView::new(&archive, &town)
                .model_instances(&town)
                .into_iter()
                .filter(|(_, _, i, _)| !i.is_empty())
                .filter_map(|(_, g, _, _)| g.first)
                .collect();
            assert!(
                !worn.is_empty() && worn.iter().all(|m| m.contains("Nomad")),
                "{worn:?}"
            );
            town.construction.commands.push(
                unbanished_game::work::professions::Command::StateMessage {
                    entity: id,
                    message: 7,
                },
            );
        }
        // Back to 2, which waits for the next band at once (0xf).
        if asked && state != 0x11 {
            admitted = true;
            break;
        }
    }
    assert!(called, "the town hall never waited for a band: {states:?}");
    assert!(band >= 2, "no band came: {states:?}");
    assert!(asked, "the band never arrived: {states:?}");
    assert!(admitted, "the band was not taken in: {states:?}");
    let after = town.citizens.iter().filter(|c| c.alive).count();
    assert_eq!(after, before + band, "{before} + {band} citizens");
    assert!(town.wildlife.nomads.is_empty());
    assert!(town.construction.visitors.0 > 0.0);
    let couples = town.citizens[before..]
        .iter()
        .filter(|c| c.spouse.is_some())
        .count();
    assert!(couples >= 2);
    eprintln!("states {states:?}, band of {band}");
}

/// A trading post by the river: a merchant's boat sails up the river and docks, the
/// merchant's goods are offered, the post trades by itself on arrival, and when the stay
/// is over the boat sails off.
#[test]
fn a_merchant_comes_by_boat() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/original/WinData");
    let mut archive = Archive::default();
    for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
        let Ok(bytes) = std::fs::read(dir.join(name)) else {
            return;
        };
        archive.add(Package::parse(bytes).unwrap());
    }
    let world = crate::view::world::World::generate(
        &archive,
        &crate::view::world::WorldParams {
            seed: 1234,
            ..Default::default()
        },
    )
    .unwrap();
    let terrain = HeightGrid {
        width: world.terrain.width(),
        heights: &world.terrain.heights,
    };
    let mut rng = Random::new(1234);
    let mut town = StartingTown::create(
        &archive,
        "Game/StartCondition/Medium.rsc:resource",
        world.terrain.size,
        &terrain,
        &mut rng,
    )
    .unwrap();
    town.construction.river = world.river_paths.first().cloned().unwrap_or_default();
    assert!(!town.construction.river.is_empty(), "no river");
    let mut tool = PlacementTool::start(
        &archive,
        &mut town,
        r"Template\TradingPost.rsc:resource",
        &mut rng,
    )
    .unwrap();
    // A valid spot by the river nearest the start.
    let n = town.cells.size as i32;
    let half = (n >> 1) as f32;
    let mut spot = None;
    let mut best = i32::MAX;
    for p in &town.construction.river {
        let (x, y) = ((p[0] + half) as i32, (p[1] + half) as i32);
        if !(0..n).contains(&x) || !(0..n).contains(&y) {
            continue;
        }
        let d = (x - town.start.0).abs() + (y - town.start.1).abs();
        if d >= best {
            continue;
        }
        'search: for dy in -6..=6 {
            for dx in -6..=6 {
                for r in 0..4 {
                    tool.hover(&town, &terrain, Some((x + dx, y + dy)), r);
                    let clear = tool.rect.is_some_and(|rect| {
                        (rect.y0..=rect.y1).all(|cy| {
                            (rect.x0..=rect.x1)
                                .all(|cx| town.cells.objects[(cy * n + cx) as usize].is_none())
                        })
                    });
                    if tool.valid() && clear {
                        best = d;
                        spot = Some(((x + dx, y + dy), r));
                        break 'search;
                    }
                }
            }
        }
    }
    let (at, rotation) = spot.expect("no trading post spot by the river");
    tool.hover(&town, &terrain, Some(at), rotation);
    let id = tool.place(&archive, &mut town, rotation).unwrap();
    let builder = town.construction.professions.builder;
    town.construction
        .commands
        .push(unbanished_game::work::professions::Command::SetCount {
            profession: builder,
            count: 4,
        });
    let st = town.construction.sites[&id].template;
    let food = town.construction.templates[st]
        .trade
        .as_ref()
        .and_then(|d| d.categories.iter().copied().find(|&f| f & 0x20 != 0));
    let mut warnings = unbanished_game::events::StockWarnings::default();
    let dt = 1.0 / 30.0;
    let (mut supplied, mut called, mut docked, mut offers, mut left) =
        (false, false, false, 0, false);
    let mut boat = None;
    let mut states = Vec::new();
    let mut visits = 0;
    for _ in 0..30 * 60 * 120 {
        town.apply_commands(&terrain, &mut rng);
        town.iterate(&terrain, &mut rng, dt, 50.0);
        let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
        town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
        warnings.recompute(&town.registered_storages(), &town.materials);
        town.construction
            .jobs
            .step(unbanished_game::scheduler::ticks(dt));
        let site = town.construction.sites.get_mut(&id).unwrap();
        let state = site.machine.state;
        if states.last() != Some(&state) {
            states.push(state);
        }
        let storage = site.storage;
        if state == 1 && !supplied {
            supplied = true;
            for &(n, m) in &town.construction.templates[st].materials {
                town.storages[storage].add(m, n);
            }
        }
        if state == 2 && !called {
            // The next merchant now, trading food for logs on arrival.
            called = true;
            let tr = site.trade.as_mut().expect("no trade state");
            tr.timer = 0;
            tr.mode = 1;
            // The seed and livestock merchants bring only what is ordered: order the
            // first good from them every time.
            for o in tr.orders.iter_mut().skip(3) {
                o.goods = vec![0];
                o.state = 2;
            }
            if let (Some(f), Some(row)) =
                (food, tr.automatic.iter_mut().find(|r| Some(r.0) == food))
            {
                let _ = f;
                row.2 = 20;
            }
            let logs = (0..town.materials.len())
                .find(|&m| town.materials[m].reference.contains("RawMaterialWood"))
                .unwrap();
            town.storages[storage].add(logs, 30);
        }
        let site = &town.construction.sites[&id];
        if let Some(tr) = &site.trade {
            if let Some(b) = tr.boat {
                boat = Some(b);
            }
            offers = offers.max(tr.offers.len());
        }
        if state == 0x13 && !docked {
            docked = true;
            // The stay is over.
            town.construction
                .sites
                .get_mut(&id)
                .unwrap()
                .trade
                .as_mut()
                .unwrap()
                .timer = 0;
        }
        if docked && state == 2 {
            if let Some(b) = boat {
                if !town.wildlife.boats.contains_key(&b) {
                    // A seed or livestock merchant without an order brings nothing: the
                    // next one.
                    if offers == 0 && visits < 5 {
                        visits += 1;
                        (called, docked, boat) = (false, false, None);
                        continue;
                    }
                    left = true;
                    break;
                }
            }
        }
    }
    assert!(called, "the post was not built: {states:?}");
    assert!(boat.is_some(), "no boat came: {states:?}");
    assert!(docked || visits > 0, "the boat never docked: {states:?}");
    assert!(offers > 0, "the merchant brought nothing");
    assert!(left, "the boat did not sail off: {states:?}");
    eprintln!("states {states:?}, {offers} offers");
}

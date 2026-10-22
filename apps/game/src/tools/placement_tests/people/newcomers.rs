//! Newcomers: nomads and merchants.

use super::*;

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

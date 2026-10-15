//! Original search control flow and Random consumption; geometry is injected in both runs.
use banished_game::random::Random;
use banished_game::stock::{
    find_storage, Item, RawMaterial, Storage, StorageSearchMap, StorageSite,
};

struct Geometry(Vec<(i32, i32, i32)>);

#[test]
fn original_storage_work_facing() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/storage/facing.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("storage facing ground truth missing; skipping");
        return;
    };
    let point = |s: &str| -> [f32; 3] {
        let p: Vec<f32> = s.split(',').map(|v| v.parse().unwrap()).collect();
        [p[0], p[1], p[2]]
    };
    let mut count = 0;
    for row in data.lines() {
        let p: Vec<_> = row.split('\t').collect();
        assert_eq!(
            banished_game::character::work_facing(point(p[0]), point(p[1])).to_bits(),
            p[2].parse::<u32>().unwrap(),
            "facing case {count}"
        );
        count += 1;
    }
    assert_eq!(count, 240);
}
impl StorageSearchMap for Geometry {
    fn entrance(&mut self, entity: usize, _: &mut Random) -> i32 {
        entity as i32
    }
    fn rank(&mut self, a: i32, b: i32) -> i32 {
        if b == 1000 {
            self.0[a as usize].1
        } else {
            self.0[a as usize].0
        }
    }
    fn market_position(
        &mut self,
        entity: usize,
        _: i32,
        _: &mut Random,
    ) -> Option<(i32, [f32; 3])> {
        Some((self.0[entity].2, [entity as f32 + 1.0, 2.0, 3.0]))
    }
    fn storage_position(&mut self, entity: usize, from: i32) -> Option<(i32, [f32; 3])> {
        self.market_position(entity, from, &mut Random::new(1))
    }
}

#[test]
fn original_storage_search_and_random() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/storage/storage.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("storage emulator ground truth missing; skipping");
        return;
    };
    let mut cases = 0;
    for (line, row) in data.lines().enumerate() {
        let cols: Vec<_> = row.split('\t').collect();
        let nums = |s: &str| {
            s.split(':')
                .map(|x| x.parse::<i64>().unwrap())
                .collect::<Vec<_>>()
        };
        let materials: Vec<_> = cols[3]
            .split(',')
            .map(|s| {
                let p = nums(s);
                RawMaterial {
                    flags: p[0] as u32,
                    preferred: p[1] != 0,
                    ..Default::default()
                }
            })
            .collect();
        let mut sites = Vec::new();
        let mut storages = Vec::new();
        let mut geometry = Geometry(Vec::new());
        for (k, site) in cols[4].split(';').enumerate() {
            let fields: Vec<_> = site.split(',').collect();
            let p = nums(fields[0]);
            sites.push(StorageSite {
                entity: k,
                storage: k,
                location_flags: p[6] as u32,
                radius: (p[0] != 0).then_some(p[4] as i32),
            });
            storages.push(Storage {
                capacity: 0,
                accepts: p[5] as u32,
                items: fields[1..]
                    .iter()
                    .enumerate()
                    .map(|(m, f)| {
                        let n = nums(f);
                        Item {
                            material: m,
                            count: n[0] as i32,
                            reserved: n[1] as i32,
                        }
                    })
                    .collect(),
            });
            geometry.0.push((p[1] as i32, p[2] as i32, p[3] as i32));
        }
        let mut random = Random::new(cols[0].parse().unwrap());
        let found = find_storage(
            &sites,
            &storages,
            &materials,
            cols[1].parse().unwrap(),
            999,
            if cols[2] == "1" { 1000 } else { -1 },
            &mut geometry,
            &mut random,
        );
        let expected_site: i32 = cols[5].parse().unwrap();
        let expected_material: i32 = cols[6].parse().unwrap();
        assert_eq!(
            found.as_ref().map_or(-1, |f| f.site as i32),
            expected_site,
            "case {line}"
        );
        assert_eq!(
            found.as_ref().map_or(-1, |f| f.material as i32),
            expected_material,
            "case {line}"
        );
        if let Some(f) = found {
            assert_eq!(f.target, [expected_site as f32 + 1.0, 2.0, 3.0]);
        }
        assert_eq!(
            random.rand(),
            cols[7].parse::<u32>().unwrap(),
            "Random case {line}"
        );
        cases += 1;
    }
    assert_eq!(cases, 600);
}

#[test]
fn original_storage_geometry_and_random() {
    use banished_game::{
        activities::{EntityView, StorageGeometry, TownView},
        pathfind::regions,
        sectors::SectorGraph,
        terrain::CellType,
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/storage/geometry.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("storage geometry ground truth missing; skipping");
        return;
    };
    let mut types = Vec::new();
    let mut region = Vec::new();
    let mut graph = None;
    let empty = vec![None; 32 * 32];
    let mut count = 0;
    for row in data.lines() {
        let p: Vec<_> = row.split('\t').collect();
        if p[0] == "map" {
            types = p[1]
                .as_bytes()
                .chunks_exact(2)
                .map(|b| CellType(u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap()))
                .collect();
            region = regions(32, &types);
            graph = Some(SectorGraph::new(32, &types));
            continue;
        }
        let ints = |s: &str| {
            s.split(',')
                .map(|v| v.parse::<i32>().unwrap())
                .collect::<Vec<_>>()
        };
        let point = |s: &str| -> [f32; 3] {
            let v: Vec<_> = s.split(',').map(|v| v.parse::<f32>().unwrap()).collect();
            [v[0], v[1], v[2]]
        };
        let rect = ints(p[3]);
        let entities = vec![EntityView {
            alive: true,
            has_interact: p[2] == "1",
            rect: Some((rect[0], rect[1], rect[2], rect[3])),
            use_points: if p[4].is_empty() {
                vec![]
            } else {
                p[4].split(';').map(point).collect()
            },
            ..Default::default()
        }];
        let town = TownView {
            size: 32,
            types: &types,
            regions: &region,
            occupants: &empty,
            objects: &empty,
            entities: &entities,
            happiness: &[],
        };
        let ground = |c: i32| [(c % 32) as f32 - 15.5, (c / 32) as f32 - 15.5, 0.0];
        let mut map = StorageGeometry {
            town: &town,
            sectors: graph.as_mut().unwrap(),
            ground: &ground,
        };
        let mut random = Random::new(p[0].parse().unwrap());
        let from = p[1].parse().unwrap();
        assert_eq!(
            map.entrance(0, &mut random),
            p[5].parse::<i32>().unwrap(),
            "entrance case {count}"
        );
        let slot = map.storage_position(0, from);
        assert_eq!(
            slot.as_ref().map_or(-1, |s| s.0),
            p[6].parse::<i32>().unwrap(),
            "slot rank case {count}"
        );
        if let Some((_, pos)) = slot {
            assert_eq!(pos, point(p[7]), "slot target case {count}");
        }
        if !entities[0].use_points.is_empty() {
            let slot = map.market_position(0, from, &mut random);
            assert_eq!(
                slot.as_ref().map_or(-1, |s| s.0),
                p[8].parse::<i32>().unwrap(),
                "market rank case {count}"
            );
            if let Some((_, pos)) = slot {
                assert_eq!(pos, point(p[9]), "market target case {count}");
            }
        }
        assert_eq!(
            random.rand(),
            p[10].parse::<u32>().unwrap(),
            "geometry Random case {count}"
        );
        count += 1;
    }
    assert_eq!(count, 240);
}

#[test]
fn original_storage_ledger_actions() {
    use banished_game::stock::{consume_stored, pickup_stored};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/derived/emu/storage/ledger.tsv");
    let Ok(data) = std::fs::read_to_string(path) else {
        eprintln!("storage ledger ground truth missing; skipping");
        return;
    };
    let materials: Vec<_> = [1, 3, 11]
        .into_iter()
        .map(|size| RawMaterial {
            size,
            ..Default::default()
        })
        .collect();
    let parse = |s: &str| -> Vec<Vec<i32>> {
        if s == "-" {
            vec![]
        } else {
            s.split(',')
                .map(|e| e.split(':').map(|n| n.parse().unwrap()).collect())
                .collect()
        }
    };
    let entries = |s: &str| {
        parse(s)
            .iter()
            .map(|n| Item {
                material: n[0] as usize,
                count: n[1],
                reserved: n[2],
            })
            .collect::<Vec<_>>()
    };
    let mut cases = 0;
    for row in data.lines() {
        let p: Vec<_> = row.split('\t').collect();
        let mut source = Storage {
            capacity: 200,
            accepts: u32::MAX,
            items: entries(p[2]),
        };
        let mut carried = Storage {
            capacity: 30,
            accepts: u32::MAX,
            items: entries(p[3]),
        };
        let requests: Vec<_> = parse(p[4]).iter().map(|n| (n[0] as usize, n[1])).collect();
        let success = match p[0] {
            "0" => {
                source.add(requests[0].0, requests[0].1);
                true
            }
            "1" => {
                source.remove(requests[0].0, requests[0].1);
                true
            }
            "2" => consume_stored(&mut source, &requests, p[1] == "1"),
            "3" => pickup_stored(
                &mut source,
                &mut carried,
                &materials,
                &requests,
                p[1] == "1",
            ),
            _ => unreachable!(),
        };
        assert_eq!(success, p[5] == "0", "status case {cases}");
        assert_eq!(source.items, entries(p[6]), "source case {cases}");
        assert_eq!(carried.items, entries(p[7]), "carried case {cases}");
        assert_eq!(
            source.used(&materials),
            p[8].parse::<i32>().unwrap(),
            "occupancy case {cases}"
        );
        cases += 1;
    }
    assert_eq!(cases, 400);
}

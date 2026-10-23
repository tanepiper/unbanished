use super::*;

/// A drop-off with `redirect` whose storage is full: with another storage found, the
/// action points there and a walk is pushed; with none, the goods are left as a pile
/// (`DropPile`) and the action fails. Without `redirect` nothing happens.
#[test]
fn a_full_storage_redirects_or_drops_a_pile() {
    let n = 8;
    let types = vec![CellType(2); n * n];
    let none = vec![None; n * n];
    let zeros = vec![0u8; n * n];
    let roads = vec![0xffu8; n * n];
    let floors = crate::world::floor::Floors::new(n * n);
    let regions = vec![0u16; n * n];
    let heights = vec![0i16; (2 * n + 1) * (2 * n + 1)];
    let terrain = HeightGrid {
        width: 2 * n + 1,
        heights: &heights,
    };
    let materials = vec![RawMaterial {
        size: 1,
        ..Default::default()
    }];
    let group = AnimationGroup {
        clips: vec![],
        default_blend: 0.0,
        blends: vec![],
        blend_matrix: vec![],
        name: String::new(),
    };
    for (found, redirect) in [(true, true), (false, true), (false, false)] {
        let mut pathfinder = Pathfinder::new(n, &types);
        let mut rng = Random::new(1);
        let (mut messages, mut used) = (Vec::new(), Vec::new());
        let full = Storage {
            capacity: 0,
            accepts: u32::MAX,
            items: vec![],
        };
        let mut storages = vec![
            full.clone(),
            Storage {
                capacity: 10,
                ..full
            },
        ];
        let mut carried = Storage {
            capacity: 10,
            accepts: u32::MAX,
            items: vec![],
        };
        carried.add(0, 3);
        let (mut tool_uses, mut shown, mut accident, mut timer) = (0, -1, None, 0);
        let mut animating = vec![];
        let mut search = |_: &[Storage], _: &mut Random, m: usize, count: i32, _: i32| {
            assert_eq!((m, count), (0, 3));
            found.then_some((1, [1.0, 1.0, 0.0]))
        };
        let mut world = World {
            size: n,
            types: &types,
            objects: &none,
            occupants: &none,
            flags: &zeros,
            roads: &roads,
            floors: &floors,
            regions: &regions,
            terrain: &terrain,
            pathfinder: &mut pathfinder,
            rng: &mut rng,
            messages: &mut messages,
            used: &mut used,
            storages: &mut storages,
            materials: &materials,
            carried: &mut carried,
            storage_search: Some(&mut search),
            pushes: Vec::new(),
            tool_uses: &mut tool_uses,
            shown_tool: &mut shown,
            animating: &mut animating,
            accident: &mut accident,
            accident_timer: &mut timer,
            accident_odds: None,
        };
        let mut ch = Character::new(CharacterDescription::default(), [0.0; 3], 1.0);
        let mut action = Action::DropOff {
            storage: 0,
            material: usize::MAX,
            count: -1,
            redirect,
        };
        let status = action.start(&mut ch, &mut world, &group);
        let pushed = world.pushes.len();
        let Action::DropOff { storage, .. } = action else {
            unreachable!()
        };
        match (found, redirect) {
            (true, _) => {
                assert_eq!((status, storage, pushed), (Status::Running, 1, 1));
                assert_eq!(world.carried.available(0), 3);
            }
            (false, true) => {
                assert_eq!((status, storage, pushed), (Status::Failed, 0, 0));
                assert_eq!(world.carried.available(0), 0);
                assert_eq!(world.messages[0].component, Component::DropPile);
                assert_eq!((world.messages[0].a, world.messages[0].b), (0, 3));
            }
            (false, false) => {
                assert_eq!((status, pushed), (Status::Running, 0));
                assert_eq!(world.carried.available(0), 3);
                assert!(world.messages.is_empty());
            }
        }
    }
}

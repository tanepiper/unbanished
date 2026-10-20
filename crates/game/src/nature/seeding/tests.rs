use super::*;
use crate::scheduler::ticks;
use unbanished_formats::game::ModelVariant;

/// FUN_1400a5750: the occupant answers for the cell before the object. An orchard is the
/// occupant of the cells its trees are the objects of; a construction site is an occupant
/// with no object until its clearing is done.
#[test]
fn cell_entity_is_the_occupant_then_the_object() {
    let mut cells = Cells::new(2, vec![CellType(0); 4]);
    assert_eq!(cells.entity_at(0), None);
    cells.objects[0] = Some(7);
    assert_eq!(cells.entity_at(0), Some(7));
    cells.occupants[0] = Some(3);
    assert_eq!(cells.entity_at(0), Some(3));
    cells.occupants[1] = Some(3);
    assert_eq!(cells.entity_at(1), Some(3));
}

fn template() -> Template {
    Template {
        reference: "test:resource".into(),
        bucket: 3,
        map: Some(MapDescription {
            cell_type: 4,
            footprint: String::new(),
            stamp: true,
            no_stamp: false,
            minimap: false,
            color: 0,
        }),
        model: Some(ModelDescription {
            variants: vec![
                ModelVariant {
                    animations: None,
                    meshes: vec![None; 3]
                };
                2
            ],
            variant: 0,
            mesh: 0,
            random_variant: true,
        }),
        natural: None,
        fruit: None,
        animations: Vec::new(),
        audio_events: Vec::new(),
        growth: Some(GrowthDescription {
            max_base: 4.0,
            max_range: 1.0,
            f28: 2.0,
            b2c: false,
            f30: 60.0,
            f34: 20.0,
            b38: false,
            f3c: 0.5,
            f40: 0.5,
            b44: true,
            spawn_radius: 1,
            timer_months: 1,
            spawn_one_in: 1,
            threshold: 0.75,
            max_neighbours: 8,
            max_same: 8,
            f60: 0.0,
            tall: false,
        }),
        growth_reference: Some("test:growth".into()),
        spawn: Vec::new(),
        updates: vec![ScheduledComponent::Growth],
        size: None,
        storage: None,
        storage_location: false,
        storage_location_flags: 0,
        storage_display: false,
        market_radius: None,
        market_location: false,
        has_interact: false,
        use_points: Vec::new(),
        pile: None,
        pile_material: None,
        pile_yield: (0, 0),
        build_points: Vec::new(),
        use_yaws: Vec::new(),
        create_points: Vec::new(),
        named_points: Vec::new(),
        floor: None,
        floor_on_activate: false,
        terraform: None,
        tornado: None,
        quarry: None,
        trading_post: false,
        tracker: false,
        fenced_area: false,
    }
}

fn parent() -> Placed {
    Placed {
        template: 0,
        bucket: 3,
        cell: (8, 8),
        footprint: (1, 1),
        position: [0.5, 0.5, 0.0],
        yaw: 0.0,
        variant: 1,
        mesh: 2,
        growth: 1.0,
        growth_state: Some(Growth {
            maximum: 4.0,
            timer: 0,
            smoothed: 1.0,
            scheduled: true,
            smoothing: false,
            allow_negative: false,
        }),
        spawn_state: None,
        alive: true,
        removal_requested: false,
        animation: None,
        scale: [1.0; 4],
    }
}

#[test]
fn periodic_children_start_at_zero_and_preserve_draw_order() {
    let templates = [template()];
    let heights = vec![0i16; 33 * 33];
    let terrain = HeightGrid {
        width: 33,
        heights: &heights,
    };
    let mut cells = Cells::new(16, vec![CellType::GROUND; 256]);
    cells.objects[8 * 16 + 8] = Some(0);
    cells.types[8 * 16 + 8] = CellType(4);
    let mut placed = vec![parent()];
    let mut removed = Vec::new();
    let mut animation_events = Vec::new();
    let mut rng = Random::new(290);
    let mut expected = rng.clone();
    // FUN_140125070's order, including the constructor's variant draw which is overwritten.
    expected.rand_float();
    let x = 8 + expected.rand_tol(1);
    let y = 8 + expected.rand_tol(1);
    assert_ne!((x, y), (8, 8));
    expected.rand_below(2);
    let maximum = expected.rand_float_tol() + 4.0;
    expected.rand_float_tol();
    expected.rand_float_tol();
    expected.rand_float_tol();
    let timer = ticks(expected.rand_float() * 300.0);
    let mesh = expected.rand_below(3) as usize;
    let mut world = Seeder {
        templates: &templates,
        terrain: &terrain,
        cells: &mut cells,
        rng: &mut rng,
        placed: &mut placed,
        removed: &mut removed,
        animation_events: &mut animation_events,
    };
    let child = world.periodic_spawn(0).unwrap();
    assert_eq!(world.placed[child].cell, (x, y));
    assert_eq!(world.placed[child].variant, 1);
    assert_eq!(world.placed[child].mesh, mesh);
    assert_eq!(world.placed[child].growth, 0.0);
    let state = world.placed[child].growth_state.as_ref().unwrap();
    assert_eq!(state.maximum.to_bits(), maximum.to_bits());
    assert_eq!(state.timer, timer);
    assert!(!state.smoothing);
    assert_eq!(world.rng.rand(), expected.rand());
    assert_eq!(
        world.cells.objects[y as usize * 16 + x as usize],
        Some(child)
    );
}

#[test]
fn placement_gates_reject_before_constructor_draws() {
    let mut templates = [template()];
    templates[0].growth.as_mut().unwrap().spawn_radius = 0;
    let heights = vec![0i16; 33 * 33];
    let terrain = HeightGrid {
        width: 33,
        heights: &heights,
    };
    let mut cells = Cells::new(16, vec![CellType::GROUND; 256]);
    let mut placed = vec![parent()];
    let mut removed = Vec::new();
    let mut animation_events = Vec::new();
    let mut rng = Random::new(290);
    let mut world = Seeder {
        templates: &templates,
        terrain: &terrain,
        cells: &mut cells,
        rng: &mut rng,
        placed: &mut placed,
        removed: &mut removed,
        animation_events: &mut animation_events,
    };
    for gate in 0..3 {
        world.cells.types[136] = if gate == 0 {
            CellType(4)
        } else {
            CellType::GROUND
        };
        world.cells.flags[136] = if gate == 1 { 1 } else { 0 };
        world.cells.occupants[136] = if gate == 2 { Some(0) } else { None };
        let mut expected = world.rng.clone();
        expected.rand_float();
        expected.rand_tol(0);
        expected.rand_tol(0);
        assert!(world.periodic_spawn(0).is_none());
        assert_eq!(world.rng.rand(), expected.rand());
    }
    assert_eq!(world.placed.len(), 1);
}

#[test]
fn newborns_wait_until_next_bucket_visit_and_removal_clears_occupancy() {
    let mut templates = [template()];
    templates[0].growth.as_mut().unwrap().max_range = 0.0;
    let heights = vec![0i16; 33 * 33];
    let terrain = HeightGrid {
        width: 33,
        heights: &heights,
    };
    let mut cells = Cells::new(16, vec![CellType::GROUND; 256]);
    cells.types[136] = CellType(4);
    cells.objects[136] = Some(0);
    let mut placed = vec![parent()];
    let mut removed = Vec::new();
    let mut animation_events = Vec::new();
    let mut rng = Random::new(290);
    let mut scheduler = EntityScheduler::default();
    scheduler.register(0, 3);
    let mut world = Seeder {
        templates: &templates,
        terrain: &terrain,
        cells: &mut cells,
        rng: &mut rng,
        placed: &mut placed,
        removed: &mut removed,
        animation_events: &mut animation_events,
    };
    assert!(world.iterate(&mut scheduler, 901.0, 60.0, false));
    assert!(world.placed.len() > 1);
    assert!(world.placed[1..].iter().all(|p| p.growth == 0.0));
    world.iterate(&mut scheduler, 1.0, 60.0, false);
    assert!(world.placed[1..].iter().all(|p| p.growth > 0.0));
    world.placed[0].growth_state.as_mut().unwrap().maximum = 1.0;
    world.iterate(&mut scheduler, 1.0, 60.0, false);
    assert!(!world.placed[0].alive);
    assert_eq!(world.cells.objects[136], None);
    assert_eq!(world.cells.types[136], CellType::GROUND);
    assert_eq!(world.cells.previous_template[136], Some(0));
    assert!(!scheduler.visit_bucket(3).iter().any(|v| v.1 == 0));
}

/// A grown parent's SpawnComponent tries a child of another template every child growth
/// period (`+0x28 × 300`) while its growth is in the entry's band; never when disabled.
#[test]
fn spawn_component_seeds_other_templates_in_band() {
    let mut tree = template();
    tree.spawn = vec![(1, 0.5, 2.0)];
    tree.updates = vec![ScheduledComponent::Growth, ScheduledComponent::Spawn];
    tree.growth.as_mut().unwrap().spawn_one_in = 1_000_000;
    let mut herb = template();
    herb.growth.as_mut().unwrap().f28 = 0.01;
    herb.growth.as_mut().unwrap().spawn_radius = 3;
    let templates = [tree, herb];
    let heights = vec![0i16; 33 * 33];
    let terrain = HeightGrid {
        width: 33,
        heights: &heights,
    };
    for disabled in [false, true] {
        let mut cells = Cells::new(16, vec![CellType::GROUND; 256]);
        cells.types[136] = CellType(4);
        cells.objects[136] = Some(0);
        let mut p = parent();
        p.spawn_state = Some(Spawn::new(1));
        p.growth_state.as_mut().unwrap().maximum = 100.0;
        let mut placed = vec![p];
        let mut removed = Vec::new();
        let mut animation_events = Vec::new();
        let mut rng = Random::new(290);
        let mut scheduler = EntityScheduler::default();
        scheduler.register(0, 3);
        let mut world = Seeder {
            templates: &templates,
            terrain: &terrain,
            cells: &mut cells,
            rng: &mut rng,
            placed: &mut placed,
            removed: &mut removed,
            animation_events: &mut animation_events,
        };
        for _ in 0..10 {
            world.iterate(&mut scheduler, 3.0, 60.0, disabled);
        }
        let herbs: Vec<&Placed> = world.placed.iter().filter(|p| p.template == 1).collect();
        if disabled {
            assert!(herbs.is_empty());
        } else {
            assert!(!herbs.is_empty());
            assert!(herbs
                .iter()
                .all(|h| h.bucket == 3 && (h.cell.0 - 8).abs() <= 3 && (h.cell.1 - 8).abs() <= 3));
        }
        // Out of band, the clock stands still.
        world.placed[0].growth = 0.2;
        let timer = world.placed[0].spawn_state.as_ref().unwrap().timers[0];
        world.iterate(&mut scheduler, 3.0, 60.0, disabled);
        assert_eq!(
            world.placed[0].spawn_state.as_ref().unwrap().timers[0],
            timer
        );
    }
}

use super::*;

/// A walk restored from a save mid-way (index 1 of 3 cells) goes on with the clip and the
/// direction the character was saved with: the save's animation state holds the walk, so the
/// first update neither plays a clip nor turns the character.
#[test]
fn a_restored_walk_keeps_its_saved_clip_and_direction() {
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
    let group = AnimationGroup {
        clips: vec![],
        default_blend: 0.0,
        blends: vec![],
        blend_matrix: vec![],
        name: String::new(),
    };
    let mut pathfinder = Pathfinder::new(n, &types);
    let mut rng = Random::new(1);
    let (mut messages, mut used) = (Vec::new(), Vec::new());
    let mut storages = vec![];
    let mut carried = Storage {
        capacity: 0,
        accepts: u32::MAX,
        items: vec![],
    };
    let (mut tool_uses, mut shown, mut accident, mut timer) = (0, -1, None, 0);
    let mut animating = vec![];
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
        materials: &[],
        carried: &mut carried,
        storage_search: None,
        pushes: Vec::new(),
        tool_uses: &mut tool_uses,
        shown_tool: &mut shown,
        animating: &mut animating,
        accident: &mut accident,
        accident_timer: &mut timer,
        accident_odds: None,
    };
    let (here, next, end) = (2 * n as i32 + 2, 2 * n as i32 + 3, 2 * n as i32 + 4);
    let (walk, stand) = (5, 6);
    let mut ch = Character::new(
        CharacterDescription::default(),
        world.cell_centre(here),
        1.0,
    );
    ch.direction = [0.0, 1.0, 0.0];
    ch.play(walk, &group, 1.0);
    let mut m = MoveTo::restored(
        world.cell_centre(end),
        walk,
        stand,
        0,
        0,
        1.0,
        false,
        1,
        &[here, next, end],
    );
    assert_eq!(m.update(&mut ch, &mut world, &group), Status::Running);
    assert_eq!(ch.animation.clip(), Some(walk));
    assert_eq!(ch.direction, [0.0, 1.0, 0.0]);
}

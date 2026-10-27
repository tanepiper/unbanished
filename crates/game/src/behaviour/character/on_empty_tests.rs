use super::*;

fn think() -> Action {
    Action::SendMessage(Message {
        component: Component::CitizenAi,
        message: 0,
        entity: None,
        a: 0,
        b: 0,
    })
}

/// The "on empty" chain sends its message when the stack runs empty. A chain set while that
/// message is handled is deleted with the old one (`FUN_1400ca080`). One set at any other time
/// stays.
#[test]
fn a_chain_set_by_the_on_empty_message_is_lost() {
    let n = 4;
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
    let mut carried = Storage::default();
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
    let mut ch = Character::new(CharacterDescription::default(), [0.0; 3], 1.0);
    let mut stack = ActionStack::default();
    stack.set_on_empty(vec![think()]);
    stack.end_on_empty();
    assert!(
        stack.saved().1.is_some(),
        "set outside a run, the chain stays"
    );
    stack.run(&mut ch, &mut world, &group, 0.0);
    assert_eq!(world.messages.len(), 1);
    assert!(stack.saved().1.is_none());
    // The message's handler sets a new chain; the run's end deletes it.
    stack.set_on_empty(vec![think()]);
    stack.end_on_empty();
    assert!(stack.saved().1.is_none());
    // A chain set later stays.
    stack.set_on_empty(vec![think()]);
    stack.end_on_empty();
    assert!(stack.saved().1.is_some());
}

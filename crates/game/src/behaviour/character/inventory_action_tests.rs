use super::*;

#[test]
fn clearing_pending_consumption_and_pickup_releases_reservations() {
    let group = AnimationGroup {
        clips: vec![],
        default_blend: 0.0,
        blends: vec![],
        blend_matrix: vec![],
    };
    let mut ch = Character::new(CharacterDescription::default(), [0.0; 3], 1.0);
    let mut storages = vec![Storage {
        capacity: 10,
        accepts: u32::MAX,
        items: vec![],
    }];
    storages[0].add(0, 3);
    let mut stack = ActionStack::default();
    for pickup in [false, true] {
        storages[0].reserve(0, 1);
        stack.push(
            &mut ch,
            &group,
            if pickup {
                Action::PickupStoredRawMaterial {
                    storage: 0,
                    items: vec![(0, 1)],
                    partial: false,
                }
            } else {
                Action::ConsumeStoredRawMaterial {
                    storage: 0,
                    items: vec![(0, 1)],
                    reserved: true,
                }
            },
        );
    }
    assert_eq!(storages[0].available(0), 1);
    stack.clear(&mut ch, &group, &mut storages);
    assert_eq!(storages[0].available(0), 3);
    assert_eq!(storages[0].entry(0).unwrap().reserved, 0);
    assert!(!stack.is_active());
}

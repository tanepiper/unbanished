use super::*;

#[test]
fn priority_weights_the_share_left() {
    let mut p = Place {
        entity: 0,
        profession: None,
        entrance: 0,
        left: 4,
        total: 4,
        candidates: BTreeSet::new(),
    };
    assert_eq!(p.priority(100), 100);
    p.left = 1;
    assert_eq!(p.priority(100), 25);
    assert_eq!(p.priority(FAR), FAR / 4);
}

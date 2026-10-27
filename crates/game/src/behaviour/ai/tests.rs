use super::*;

#[test]
fn qsort_orders_by_priority_descending() {
    let mut v: Vec<Candidate> = (0..20).map(|i| (i, (i as i32 * 7) % 5)).collect();
    crt_qsort(&mut v);
    assert!(v.windows(2).all(|w| w[0].1 >= w[1].1), "{v:?}");
    let mut v: Vec<Candidate> = vec![(12, 0), (8, 5), (3, 3), (9, 2)];
    crt_qsort(&mut v);
    assert_eq!(v, vec![(8, 5), (3, 3), (9, 2), (12, 0)]);
}

#[test]
fn new_adult_candidates() {
    let ai = CitizenAi::default();
    let view = CitizenView {
        age: 20.0,
        health: 1.0,
        ..Default::default()
    };
    let d = Thresholds {
        adult_age: 10.0,
        hungry: 50.0,
        hungry_urgent: 75.0,
        cold: 50.0,
        cold_urgent: 75.0,
        ..Default::default()
    };
    let (c, shuffle) = candidates(&ai, &view, &d, false);
    assert!(shuffle);
    assert_eq!(c, vec![(CLOTHING, 3), (IDLE, 0), (TOOL, 5), (WORK, 2)]);
}

/// `FUN_1401148b0`: a NaN hunger is not urgent (`jbe` at 0x140114a1b), and it ends an urgent
/// meal (`ja` at 0x140114b49 not taken). The same holds for the cold.
#[test]
fn a_nan_hunger_or_cold_is_not_urgent() {
    let d = Thresholds {
        hungry_urgent: 75.0,
        cold_urgent: 75.0,
        ..Default::default()
    };
    let view = CitizenView {
        hunger: f32::NAN,
        cold: f32::NAN,
        health: 1.0,
        ..Default::default()
    };
    let ai = CitizenAi::default();
    assert!(!ai.hunger_branch(0, &view, &d));
    let urgent = CitizenView {
        hunger: 80.0,
        ..view
    };
    assert!(ai.hunger_branch(0, &urgent, &d));
    let mut ai = CitizenAi {
        eating: true,
        warming: true,
        ..Default::default()
    };
    ai.update(
        0,
        &view,
        &d,
        &mut Random::new(1),
        None,
        || true,
        |_, _, _| false,
    );
    assert!(!ai.eating && !ai.warming);
}

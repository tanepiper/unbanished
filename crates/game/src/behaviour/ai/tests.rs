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

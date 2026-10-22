use super::*;

/// A host whose works answer from a table and record the order they were offered.
struct Host {
    rng: Random,
    answers: BTreeMap<i32, u8>,
    offered: Vec<i32>,
    profession: usize,
    workplace: Option<usize>,
    stock: [(i32, i32); 21],
    cells: BTreeMap<i32, i32>,
}

impl WorkHost for Host {
    fn profession(&self, _: usize) -> usize {
        self.profession
    }
    fn workplace(&self, _: usize) -> Option<usize> {
        self.workplace
    }
    fn cell(&self, _: usize) -> i32 {
        0
    }
    fn has_emergency(&self, _: usize) -> bool {
        false
    }
    fn works_anywhere(&self, p: usize) -> bool {
        p < 2
    }
    fn rank(&mut self, _: i32, b: i32) -> i32 {
        self.cells.get(&b).copied().unwrap_or(b)
    }
    fn stock(&self, slot: usize) -> (i32, i32) {
        self.stock[slot]
    }
    fn laborer(&self) -> usize {
        0
    }
    fn builder(&self) -> usize {
        1
    }
    fn rng(&mut self) -> &mut Random {
        &mut self.rng
    }
    fn refresh(&mut self, _: &mut Work) {}
    fn try_assign(&mut self, work: &mut Work, _: usize, _: i64) -> u8 {
        self.offered.push(work.id);
        self.answers.get(&work.id).copied().unwrap_or(1)
    }
    fn complete(&mut self, _: &mut Work, _: usize, _: i32) -> bool {
        true
    }
    fn fail(&mut self, _: &mut Work, _: usize, _: i32) {}
    fn cancel(&mut self, _: &mut Work) {}
}

fn host() -> Host {
    Host {
        rng: Random::new(7),
        answers: BTreeMap::new(),
        offered: Vec::new(),
        profession: 0,
        workplace: None,
        stock: [(100, 100); 21],
        cells: BTreeMap::new(),
    }
}

#[test]
fn ids_priorities_and_offer_order() {
    let mut jobs = Jobs::default();
    let mut h = host();
    let a = jobs.add(Work::new(5, 10, 0, WorkKind::Test(0)), 1 << 13, None);
    let b = jobs.add(Work::new(6, 20, 0, WorkKind::Test(0)), 1 << 13, Some(0));
    assert_eq!((a, b), (1, 2));
    // The kept priority 0 is taken by `a`, so `b` moves down to -1 and is offered first.
    assert_eq!(jobs.works[&b].priority, -1);
    jobs.assign_work(0, &mut h);
    assert_eq!(h.offered, vec![b, a]);
    // Both answered "not now": skipped for the rest of this pass.
    h.offered.clear();
    jobs.assign_work(0, &mut h);
    assert!(h.offered.is_empty());
    jobs.step(1);
    h.answers.insert(a, 2);
    jobs.assign_work(0, &mut h);
    assert_eq!(h.offered, vec![b, a]);
    jobs.remove(a, &mut h);
    assert_eq!(jobs.works.len(), 1);
}

#[test]
fn builders_try_construction_by_distance() {
    let mut jobs = Jobs::default();
    let mut h = host();
    h.profession = 1;
    let near = jobs.add(Work::new(5, 30, 1, WorkKind::Test(0)), 1 << 13, None);
    let far = jobs.add(Work::new(6, 3, 1, WorkKind::Test(0)), 1 << 13, None);
    h.cells.insert(30, 50);
    h.cells.insert(3, 40);
    // One list (construction): Rand(1) is drawn, then the works by rank 40, 50.
    let mut check = Random::new(7);
    jobs.assign_emergency(0, &mut h);
    check.rand_below(1);
    assert_eq!(h.offered, vec![far, near]);
    assert_eq!(h.rng.rand(), check.rand());
}

#[test]
fn no_list_draws_nothing() {
    let mut jobs = Jobs::default();
    let mut h = host();
    jobs.add(Work::new(5, 30, 0, WorkKind::Test(0)), 1 << 10, None);
    assert!(!jobs.assign_emergency(0, &mut h));
    assert_eq!(h.rng.rand(), Random::new(7).rand());
}

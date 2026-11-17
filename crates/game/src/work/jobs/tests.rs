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
    /// The player of the entities a work is at, and the citizen's own (0 unless a test sets it).
    entity_owners: BTreeMap<usize, u8>,
    citizen_owner: u8,
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
    fn stock(&self, _citizen: usize, slot: usize) -> (i32, i32) {
        self.stock[slot]
    }
    fn entity_owner(&self, entity: usize) -> u8 {
        self.entity_owners.get(&entity).copied().unwrap_or(0)
    }
    fn citizen_owner(&self, _citizen: usize) -> u8 {
        self.citizen_owner
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
        entity_owners: BTreeMap::new(),
        citizen_owner: 0,
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

#[test]
fn a_citizen_is_offered_only_its_players_work() {
    let mut jobs = Jobs::default();
    let mut h = host();
    h.entity_owners.insert(6, 1);
    let mine = jobs.add(Work::new(5, 10, 0, WorkKind::Test(0)), 1 << 13, None);
    jobs.add(Work::new(6, 20, 0, WorkKind::Test(0)), 1 << 13, None);
    // Player 0's citizen: the other player's workplace is not on its list.
    jobs.assign_work(0, &mut h);
    assert_eq!(h.offered, vec![mine]);
    // Player 1's citizen sees that one only.
    h.offered.clear();
    h.citizen_owner = 1;
    jobs.step(1);
    jobs.assign_work(0, &mut h);
    assert_eq!(h.offered.len(), 1);
    assert_ne!(h.offered[0], mine);
}

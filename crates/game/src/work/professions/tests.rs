use super::*;

struct Flat;
impl StorageSearchMap for Flat {
    fn entrance(&mut self, entity: usize, _rng: &mut Random) -> i32 {
        entity as i32
    }
    fn rank(&mut self, a: i32, b: i32) -> i32 {
        (a - b).abs()
    }
    fn market_position(&mut self, _: usize, _: i32, _: &mut Random) -> Option<(i32, [f32; 3])> {
        None
    }
    fn storage_position(&mut self, _: usize, _: i32) -> Option<(i32, [f32; 3])> {
        None
    }
}

fn professions() -> Professions {
    Professions {
        list: (0..3)
            .map(|i| Profession {
                death: String::new(),
                accident: (0.0, 0.0),
                works_anywhere: i < 2,
                immune_at_work: false,
                ..Default::default()
            })
            .collect(),
        laborer: 0,
        builder: 1,
    }
}

#[test]
fn spinner_promotes_the_first_laborers_and_demotes_unattached_first() {
    let p = professions();
    let mut workers = vec![
        Worker {
            adult: false,
            profession: 0,
            workplace: None,
            home: None,
        };
        1
    ];
    workers.extend((0..4).map(|_| Worker {
        adult: true,
        profession: 0,
        workplace: None,
        home: None,
    }));
    let mut force = Workforce::default();
    force.workplaces.insert(10, WorkPlace::new(None, 1));
    force.workplaces.get_mut(&10).unwrap().profession = Some(1);
    force.register(10, 1);
    let mut rng = Random::new(1);
    force.set_count(
        &mut workers,
        &p,
        1,
        2,
        &BTreeMap::new(),
        &mut Flat,
        &mut rng,
    );
    // The first builder fills the site's free place; the second has no workplace.
    assert_eq!(workers[1].profession, 1);
    assert_eq!(workers[1].workplace, Some(10));
    assert_eq!(workers[2].profession, 1);
    assert_eq!(workers[2].workplace, None);
    assert_eq!(force.counts(&workers, 3), vec![2, 2, 0]);
    assert_eq!(force.capacity(1), 1);
    // Demotion takes the builder without a workplace first.
    force.set_count(
        &mut workers,
        &p,
        1,
        1,
        &BTreeMap::new(),
        &mut Flat,
        &mut rng,
    );
    assert_eq!(workers[2].profession, 0);
    assert_eq!(workers[1].profession, 1);
    force.set_count(
        &mut workers,
        &p,
        1,
        0,
        &BTreeMap::new(),
        &mut Flat,
        &mut rng,
    );
    assert_eq!(workers[1].profession, 0);
    assert_eq!(workers[1].workplace, None);
    assert!(force.workplaces[&10].workers.is_empty());
}

/// Every workplace reachable except entity 99; priorities by entity.
struct Open(Random);
impl Search for Open {
    fn cell(&self, c: usize) -> i32 {
        c as i32
    }
    fn anywhere(&self, profession: usize) -> bool {
        profession == 1
    }
    fn priority(&self, workplace: usize) -> i32 {
        [0, 30, 20, 10][workplace % 4]
    }
    fn reach(&mut self, _cell: i32, workplace: usize) -> bool {
        workplace != 99
    }
    fn rng(&mut self) -> &mut Random {
        &mut self.0
    }
}

fn workforce(places: &[(usize, usize, i32, i32)]) -> Workforce {
    let mut w = Workforce::default();
    for &(e, p, max, workers) in places {
        let mut wp = WorkPlace::new(Some(p), max);
        wp.profession = Some(p);
        wp.workers = (100..100 + workers as usize).collect();
        w.workplaces.insert(e, wp);
        w.register(e, p);
    }
    w
}

#[test]
fn workplace_searches() {
    let mut search = Open(Random::new(1));
    // Most room, the first of equals; full, disabled and unreachable ones are skipped.
    let mut w = workforce(&[(1, 2, 4, 3), (2, 2, 4, 1), (3, 2, 5, 2), (99, 2, 9, 0)]);
    assert_eq!(w.find_workplace(0, 2, &mut search), Some(2));
    w.workplaces.get_mut(&2).unwrap().enabled = false;
    assert_eq!(w.find_workplace(0, 2, &mut search), Some(3));
    // Works anywhere: the best priority with room.
    let w = workforce(&[(1, 1, 2, 0), (2, 1, 2, 0), (3, 1, 2, 2)]);
    assert_eq!(w.find_workplace(0, 1, &mut search), Some(2));
    // Balancing: move when the room differs by more than one.
    let w = workforce(&[(1, 2, 4, 4), (2, 2, 4, 3)]);
    assert!(!w.should_move(0, 2, Some(1), &mut search));
    let w = workforce(&[(1, 2, 4, 4), (2, 2, 4, 2)]);
    assert!(w.should_move(0, 2, Some(1), &mut search));
    // Works anywhere: move to work that comes first, or off an unreachable workplace.
    let w = workforce(&[(1, 1, 2, 1), (3, 1, 2, 0), (99, 1, 2, 1)]);
    assert!(w.should_move(0, 1, Some(1), &mut search));
    assert!(!w.should_move(0, 1, Some(3), &mut search));
    assert!(w.should_move(0, 1, Some(99), &mut search));
}

#[test]
fn evicted_workers_move_on() {
    let mut search = Open(Random::new(1));
    let mut w = workforce(&[(1, 2, 4, 0), (2, 2, 4, 0)]);
    let mut workers: Vec<Worker> = (0..3)
        .map(|_| Worker {
            adult: true,
            profession: 2,
            workplace: None,
            home: None,
        })
        .collect();
    for c in 0..3 {
        w.set_workplace(&mut workers, c, Some(1));
    }
    w.set_max_workers(&mut workers, 1, 1, &mut search);
    assert_eq!(w.workplaces[&1].workers, vec![2]);
    assert_eq!(w.workplaces[&2].workers, vec![0, 1]);
    w.set_max_workers(&mut workers, 1, 2, &mut search);
    w.set_enabled(&mut workers, 2, false, false, &mut search);
    assert_eq!(w.workplaces[&1].workers, vec![2, 0]);
    assert!(w.workplaces[&2].workers.is_empty());
    assert_eq!(workers[1].workplace, None, "no room left: no workplace");
}

#[test]
fn workplace_status_icons() {
    let mut search = Open(Random::new(1));
    let mut w = workforce(&[(1, 2, 2, 0)]);
    let mut workers: Vec<Worker> = (0..2)
        .map(|_| Worker {
            adult: true,
            profession: 2,
            workplace: None,
            home: None,
        })
        .collect();
    let bit = |w: &Workforce, b: u8| w.workplaces[&1].status >> b & 1 != 0;
    // Enabled without workers of its own profession: WorkersUnavailable.
    w.set_enabled(&mut workers, 1, true, false, &mut search);
    assert!(bit(&w, icon::NO_WORKERS));
    w.set_workplace(&mut workers, 0, Some(1));
    assert!(!bit(&w, icon::NO_WORKERS));
    w.workplaces
        .get_mut(&1)
        .unwrap()
        .set_icon(icon::LIMIT, true);
    // The last worker leaving drops the limit icon and shows the workers icon again.
    w.set_workplace(&mut workers, 0, None);
    assert!(!bit(&w, icon::LIMIT) && bit(&w, icon::NO_WORKERS));
    // Switched off: WorkDisabled (BuildPaused while being built), nothing else.
    w.set_enabled(&mut workers, 1, false, false, &mut search);
    assert_eq!(w.workplaces[&1].status, 1 << icon::WORK_DISABLED);
    w.set_enabled(&mut workers, 1, true, false, &mut search);
    w.set_enabled(&mut workers, 1, false, true, &mut search);
    assert_eq!(w.workplaces[&1].status, 1 << icon::BUILD_PAUSED);
}

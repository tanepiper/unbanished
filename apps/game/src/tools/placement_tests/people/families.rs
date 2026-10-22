//! Families: moving in, children, hunger and school.

use super::*;

/// A finished wood house gets a family, who eat from the town's storage, carry food home
/// and have a child there.
#[test]
fn a_family_moves_in_stocks_the_house_and_has_a_child() {
    with_town(|archive, terrain, town, rng| {
        let mut tool =
            PlacementTool::start(archive, town, r"Template\WoodHouse.rsc:resource", rng).unwrap();
        let spot = tool.find_clear_spot(town, terrain, town.start).unwrap();
        tool.hover(town, terrain, Some(spot), 0);
        let id = tool.place(archive, town, 0).unwrap();
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::SetCount {
                profession: builder,
                count: 2,
            });
        let mut built = false;
        for _ in 0..30 * 60 * 8 {
            step(town, terrain, rng, 50.0);
            if town.construction.sites[&id].complete {
                built = true;
                break;
            }
        }
        assert!(built, "the house was not built");
        // Hungry enough to eat (`D[0xc8]`), not yet urgently.
        let hungry = town.agents.life.f(0xc8) + 1.0;
        for s in &mut town.agents.states {
            s.hunger = hungry;
        }
        let mut moved_in = None;
        for k in 0..30 * 60 * 3 {
            step(town, terrain, rng, 50.0);
            if moved_in.is_none() && town.construction.homes[&id].occupants() > 0 {
                moved_in = Some(k);
            }
            let home = &town.construction.homes[&id];
            if moved_in.is_some()
                && unbanished_game::people::life::stored(
                    &town.storages[home.storage],
                    &town.materials,
                    5,
                ) > 0
                && town.agents.states.iter().any(|s| !s.foods.is_empty())
            {
                break;
            }
        }
        let home = town.construction.homes[&id].clone();
        eprintln!("moved in after {moved_in:?}: {:?}", home.families);
        assert!(moved_in.is_some(), "nobody moved in");
        let family = &home.families[0];
        for &c in family {
            assert_eq!(town.agents.states[c].home, Some(id));
        }
        let food =
            unbanished_game::people::life::stored(&town.storages[home.storage], &town.materials, 5);
        eprintln!("food at home: {food}");
        assert!(food > 0, "nobody carried food home");
        assert!(
            town.agents.states.iter().any(|s| !s.foods.is_empty()),
            "nobody ate"
        );
        assert!(town.citizens.iter().all(|c| c.alive), "someone died");
        // A married woman of the family: her pregnancy ends at once.
        let mother = family
            .iter()
            .copied()
            .find(|&c| {
                let ci = &town.citizens[c];
                !ci.male && ci.spouse.is_some() && ci.age >= town.rules.adult_age
            })
            .expect("no married woman in the house");
        let before = town.citizens.len();
        town.agents.states[mother].reproduction = 0.001;
        town.agents.states[mother].health = 10.0;
        for _ in 0..10 {
            step(town, terrain, rng, 50.0);
        }
        assert_eq!(town.citizens.len(), before + 1, "no child was born");
        let child = before;
        assert_eq!(town.citizens[child].parent, Some(mother));
        assert_eq!(town.agents.states[child].home, Some(id));
        assert!(town.construction.homes[&id].families[0].contains(&child));
        assert!(town.agents.registry.contains(&child));
        assert!(
            town.agents.states[mother].reproduction < 0.0,
            "no wait after the birth"
        );
    });
}

/// Without food, hungry citizens starve once idle: the entity goes, the family and the
/// registry let go of it, and the log hears of it.
#[test]
fn citizens_starve_without_food() {
    with_town(|_, terrain, town, rng| {
        for s in &mut town.storages {
            let materials = &town.materials;
            s.items
                .retain(|e| materials[e.material].flags >> 5 & 1 == 0);
        }
        let max = town.agents.life.f(0xd0);
        for s in &mut town.agents.states {
            s.hunger = max - 0.01;
        }
        let n = town.citizens.len();
        let mut events = Vec::new();
        for _ in 0..30 * 30 {
            step(town, terrain, rng, 50.0);
            events.extend(std::mem::take(&mut town.agents.events));
            if town.citizens.iter().all(|c| !c.alive) {
                break;
            }
        }
        let dead = town.citizens.iter().filter(|c| !c.alive).count();
        eprintln!("{dead} of {n} starved");
        assert!(dead > 0, "nobody starved");
        assert_eq!(town.agents.registry.len(), n - dead);
        assert!(events
            .iter()
            .any(|e| e.kind == unbanished_game::people::life::reason::STARVED));
        for (i, c) in town.citizens.iter().enumerate().filter(|(_, c)| !c.alive) {
            assert!(c.spouse.is_none());
            assert!(!town.agents.characters[i].stack.is_active());
        }
    });
}

/// A staffed school takes a child coming of age, who sits lessons there (message 4).
#[test]
fn a_teacher_staffs_the_school_and_a_student_learns() {
    with_town(|archive, terrain, town, rng| {
        let mut tool =
            PlacementTool::start(archive, town, r"Template\SchoolHouse.rsc:resource", rng).unwrap();
        let spot = tool.find_clear_spot(town, terrain, town.start).unwrap();
        tool.hover(town, terrain, Some(spot), 0);
        let id = tool.place(archive, town, 0).unwrap();
        let builder = town.construction.professions.builder;
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::SetCount {
                profession: builder,
                count: 3,
            });
        let mut built = false;
        for _ in 0..30 * 60 * 12 {
            step(town, terrain, rng, 50.0);
            if town.construction.sites[&id].complete {
                built = true;
                break;
            }
        }
        assert!(built, "the school was not built");
        assert!(town.construction.attendance_list.contains(&id));
        let teacher = town.agents.life.services[0].expect("no teacher profession");
        town.construction
            .commands
            .push(unbanished_game::work::professions::Command::AddWorker { entity: id });
        for _ in 0..60 {
            step(town, terrain, rng, 50.0);
        }
        let staff = town.construction.workforce.workplaces[&id].workers.clone();
        assert_eq!(staff.len(), 1, "no teacher hired");
        assert_eq!(town.agents.states[staff[0]].profession, teacher);
        // A child about to come of age.
        let adult = town.rules.adult_age;
        let child = (0..town.citizens.len())
            .find(|&c| town.citizens[c].age < adult)
            .expect("no child");
        town.citizens[child].age = adult - 0.0001;
        let mut learned = false;
        for _ in 0..30 * 60 * 4 {
            step(town, terrain, rng, 50.0);
            if town.citizens[child].education > 0 {
                learned = true;
                break;
            }
        }
        eprintln!(
            "school {:?}, attendees {:?}, education {}",
            town.agents.states[child].school,
            town.construction.attendance[&id].attendees,
            town.citizens[child].education
        );
        assert_eq!(town.agents.states[child].school, Some(id));
        assert!(learned, "no lesson");
    });
}

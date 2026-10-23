//! Citizens' lives: ageing, births and deaths, and removing a citizen.

use super::*;

impl Agents {
    /// `FUN_140118340` for citizen `i`; returns the children born.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn live(
        &mut self,
        i: usize,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        entities: &[EntityView],
        dt: i64,
        newborn: &mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
    ) -> Vec<usize> {
        if !citizens[i].alive || self.dying.iter().any(|&(d, _)| d == i) {
            return Vec::new();
        }
        let was_child = citizens[i].age < self.life.adult_age();
        let mut born = Vec::new();
        self.with_people(citizens, env, entities, newborn, &mut born, |people| {
            people.update(i, dt)
        });
        // `FUN_1401195c0`: an adult's model and walk at full scale.
        if was_child && citizens[i].age >= self.life.adult_age() {
            self.characters[i].scale = 1.0;
        }
        born
    }

    /// Run `f` over the citizens' life context.
    pub(crate) fn with_people<R>(
        &mut self,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        entities: &[EntityView],
        newborn: &mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
        born: &mut Vec<usize>,
        f: impl FnOnce(&mut crate::people::life::People) -> R,
    ) -> R {
        let idle: Vec<bool> = self
            .characters
            .iter()
            .map(|c| !c.stack.is_active())
            .collect();
        let happiness = env.construction.happiness.clone();
        let town = TownView {
            size: env.cells.size,
            types: &env.cells.types,
            regions: &self.regions,
            occupants: &env.cells.occupants,
            objects: &env.cells.objects,
            floors: &env.cells.floors,
            entities,
            happiness: &happiness,
        };
        let c = &mut *env.construction;
        let mut people = crate::people::life::People {
            citizens,
            states: &mut self.states,
            registry: &mut self.registry,
            homes: &mut c.homes,
            rules: &self.life,
            town: &town,
            layers: &env.cells.layers,
            rng: env.rng,
            temperature: env.temperature,
            diseases: &self.diseases,
            town_disease: self.town_disease,
            storages: env.storages,
            materials: env.materials,
            idle: &idle,
            events: &mut self.events,
            dying: &mut self.dying,
            newborn,
            born,
            attendance: &mut c.attendance,
            attendance_list: &c.attendance_list,
            workforce: &mut c.workforce,
            professions: &c.professions,
            sites: &c.sites,
            cemeteries: &c.cemeteries,
            tombstones: &c.tombstones,
            burials: &mut c.burials,
            site_templates: &c.templates,
            sectors: &mut self.sectors,
            terrain: env.terrain,
            tree: &env.wildlife.tree,
        };
        f(&mut people)
    }

    /// `FUN_140119170`: the dead citizen's components let go of the town (a removed entity's
    /// CharacterComponent abandons its actions, CitizenComponent leaves its home, family,
    /// workplace and registry; a laborer takes over its profession).
    /// The carried goods go with it, as StorageComponent's removal slot is the default (no drop).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn remove(
        &mut self,
        c: usize,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        entities: &[EntityView],
    ) {
        // CharacterComponent's removal slot (`FUN_14010ee50`): out of the characters' tree.
        if let Some(ch) = self.characters.get_mut(c) {
            if 0 <= ch.tree_slot {
                env.wildlife.tree.remove(ch.tree_slot);
                ch.tree_slot = -1;
            }
        }
        // Abandon the action stack: aborted CompleteTasks fail their works.
        {
            let adult = self.adults(citizens);
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, entities);
            sim.clear_citizen(c, None);
            sim.characters[c].stack.set_on_empty(Vec::new());
            jobs.drain(&mut sim);
        }
        let happiness = env.construction.happiness.clone();
        let town = TownView {
            size: env.cells.size,
            types: &env.cells.types,
            regions: &self.regions,
            occupants: &env.cells.occupants,
            objects: &env.cells.objects,
            floors: &env.cells.floors,
            entities,
            happiness: &happiness,
        };
        let k = &mut *env.construction;
        let mut born = Vec::new();
        let mut never = |_: &mut Random, m: &Citizen, _| m.clone();
        let idle = vec![true; citizens.len()];
        let mut people = crate::people::life::People {
            citizens,
            states: &mut self.states,
            registry: &mut self.registry,
            homes: &mut k.homes,
            rules: &self.life,
            town: &town,
            layers: &env.cells.layers,
            rng: env.rng,
            temperature: env.temperature,
            diseases: &self.diseases,
            town_disease: self.town_disease,
            storages: env.storages,
            materials: env.materials,
            idle: &idle,
            events: &mut self.events,
            dying: &mut self.dying,
            newborn: &mut never,
            born: &mut born,
            attendance: &mut k.attendance,
            attendance_list: &k.attendance_list,
            workforce: &mut k.workforce,
            professions: &k.professions,
            sites: &k.sites,
            cemeteries: &k.cemeteries,
            tombstones: &k.tombstones,
            burials: &mut k.burials,
            site_templates: &k.templates,
            sectors: &mut self.sectors,
            terrain: env.terrain,
            tree: &env.wildlife.tree,
        };
        let workplace = people.states[c].workplace;
        let profession = people.states[c].profession;
        people.bury(c);
        let laborer = k.professions.laborer;
        people.citizens[c].alive = false;
        // `FUN_1401173b0(None)` then `Town::Hire` for anything but a laborer.
        let mut workers: Vec<crate::work::professions::Worker> = people
            .states
            .iter()
            .zip(people.citizens.iter())
            .map(|(s, ci)| crate::work::professions::Worker {
                adult: ci.alive && ci.age >= self.life.adult_age() && s.school.is_none(),
                profession: s.profession,
                workplace: s.workplace,
                home: s.home,
            })
            .collect();
        k.workforce.set_workplace(&mut workers, c, None);
        if profession != laborer && !k.professions.list.is_empty() {
            let ground_size = env.cells.size;
            let terrain = env.terrain;
            let floors = &env.cells.floors;
            let ground =
                move |cell: i32| crate::town::floor_ground(floors, ground_size, terrain, cell);
            let mut geometry = StorageGeometry {
                town: &town,
                sectors: &mut self.sectors,
                ground: &ground,
            };
            let hired = k.workforce.hire(
                &mut workers,
                laborer,
                profession,
                workplace,
                &k.homes,
                &mut geometry,
                env.rng,
            );
            match hired {
                Some(h) => self.events.push(crate::people::life::LifeEvent {
                    kind: crate::people::life::reason::REPLACED,
                    severity: 1,
                    citizen: h,
                    extra: Some((None, citizens[c].name.clone())),
                }),
                None => self.events.push(crate::people::life::LifeEvent {
                    kind: crate::people::life::reason::NOT_REPLACED,
                    severity: 1,
                    citizen: c,
                    extra: None,
                }),
            }
        }
        for (s, w) in self.states.iter_mut().zip(&workers) {
            s.profession = w.profession;
            s.workplace = w.workplace;
        }
    }
}

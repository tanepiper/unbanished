//! Delivering a citizen's messages and actions.

use super::*;

impl Agents {
    /// Deliver the messages citizen `i`'s actions sent, in order.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::behaviour::agents) fn deliver(
        &mut self,
        i: usize,
        messages: Vec<Message>,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        adult: &[bool],
        entities: &mut Vec<EntityView>,
    ) {
        for m in messages {
            // A message can create entities (piles) that a later one's AI reads.
            refresh_views(entities, env);
            let entities: &[EntityView] = entities;
            match m.component {
                Component::Citizen => match m.message {
                    0 => {
                        let home = m.entity.and_then(|e| env.construction.homes.get(&e));
                        crate::people::life::warm_at(&mut self.states[i], home, env.temperature);
                    }
                    2 => {
                        if let Some(st) = m.entity.and_then(|e| storage_of(env, e)) {
                            crate::people::life::eat(
                                &mut self.states[i],
                                &mut env.storages[st],
                                env.materials,
                                env.rng,
                                &mut env.construction.counters,
                            );
                        }
                    }
                    3 => {
                        // Treated by the herbalist or doctor.
                        self.states[i].health_events |= 0x200;
                        self.states[i].health += 0.5;
                    }
                    4 | 5 => {
                        let mut born = Vec::new();
                        self.with_people(
                            citizens,
                            env,
                            entities,
                            &mut |_, m, _| m.clone(),
                            &mut born,
                            |p| {
                                if m.message == 4 {
                                    p.lesson(i)
                                } else {
                                    p.treated(i)
                                }
                            },
                        );
                    }
                    _ => self.states[i].message(m, self.rules),
                },
                Component::CitizenAi if m.message == 0 => {
                    // `FUN_140114810`: the CitizenComponent's update with no time (while the
                    // character is idle: hunger, cold and old age can end it here), then the
                    // AI's unless the entity is going.
                    self.live(i, citizens, env, entities, 0, &mut |_, m, _| m.clone());
                    if !self.dying.iter().any(|&(d, _)| d == i) {
                        self.think(i, citizens, env, groups, adult, entities, 0);
                    }
                }
                Component::Build if m.message == 0 => {
                    // `FUN_14010dbe0`: one unit of work.
                    if let Some(site) = m.entity.and_then(|e| env.construction.sites.get_mut(&e)) {
                        site.progress += 1;
                    }
                }
                Component::Road => {
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.complete_road(m.a, m.message as u8, m.b as u8);
                    jobs.drain(&mut sim);
                }
                Component::Fire => {
                    if let Some(e) = m.entity {
                        let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                        sim.fire_message(jobs, e, m.message);
                        jobs.drain(&mut sim);
                    }
                }
                Component::Destroy if m.message == 0 => {
                    // `FUN_140122fc0`: one unit of deconstruction.
                    if let Some(d) = m
                        .entity
                        .and_then(|e| env.construction.sites.get_mut(&e))
                        .and_then(|s| s.destroy.as_mut())
                    {
                        d.count += 1;
                    }
                }
                Component::Jobs => {
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    if m.message == 0 {
                        jobs.complete(m.a, i, m.b, &mut sim);
                    } else {
                        jobs.fail(m.a, i, m.b, &mut sim);
                    }
                    jobs.drain(&mut sim);
                }
                Component::CreateRawMaterial => {
                    let educated = citizens[i].education >= self.rules.education;
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    let factor = f32::from_bits(m.message as u32);
                    sim.create_raw_material(
                        jobs,
                        i,
                        m.a.max(0) as usize,
                        m.b,
                        educated,
                        factor,
                        m.entity,
                    );
                    jobs.drain(&mut sim);
                }
                // The default message handler: nothing.
                Component::Fishing => {}
                Component::Quarry => {
                    if let Some(e) = m.entity {
                        let (mut sim, _) = self.sim(citizens, env, groups, adult, entities);
                        sim.quarry_message(e);
                    }
                }
                Component::DropPile => {
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.drop_loose_pile(i, m.a.max(0) as usize, m.b);
                    jobs.drain(&mut sim);
                }
                Component::CreateNaturalResource => {
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.plant(m.a.max(0) as usize, m.b);
                    jobs.drain(&mut sim);
                }
                Component::NaturalResource => {
                    let Some(e) = m.entity else { continue };
                    let educated = citizens[i].education >= self.rules.education;
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    match m.message {
                        0 => {
                            // `FUN_1400fe220`: a work at the resource's cell lends its priority.
                            let n = sim.size as i32;
                            let (x, y) = sim.placed[e].cell;
                            let cell = y * n + x;
                            let priority = jobs
                                .works
                                .values()
                                .find(|w| w.cell == cell)
                                .map(|w| w.priority);
                            sim.remove_resource(i, e, educated, priority, m.a != 0);
                            sim.activate_piles(jobs);
                        }
                        1 => sim.unclaim(e),
                        _ => sim.animate_resource(e, m.a.max(0) as usize),
                    }
                    jobs.drain(&mut sim);
                }
                Component::Orchard => {
                    let Some(e) = m.entity else { continue };
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.orchard_message(e, m.message, m.a);
                    jobs.drain(&mut sim);
                }
                Component::CropField => {
                    let Some(e) = m.entity else { continue };
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.crop_message(e, m.message, m.a);
                    jobs.drain(&mut sim);
                }
                Component::Animal => {
                    if let Some(a) = m.entity {
                        env.wildlife.kill(a);
                    }
                }
                Component::Pasture => {
                    let Some(e) = m.entity else { continue };
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.pasture_message(e, m.message);
                    jobs.drain(&mut sim);
                }
                Component::Happiness => {
                    let Some(e) = m.entity else { continue };
                    if m.message != 0 {
                        continue;
                    }
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.happiness_visit(e);
                    jobs.drain(&mut sim);
                }
                Component::Pile => {
                    let Some(e) = m.entity else { continue };
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.take_pile(jobs, i, e);
                    jobs.drain(&mut sim);
                }
                _ => {}
            }
        }
    }
}

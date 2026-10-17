//! Saving, stage 3c (the inverse of `load::wildlife`): populations (`FUN_140054e00`), herds
//! (`FUN_140127460`), animals (AnimalComponent `FUN_14010c310`, WildAnimalComponent,
//! LivestockComponent), pastures, the town's pasture and livestock lists, and trader boats
//! (`FUN_140046f30`). Pointers are to the components the serializers check (a herd's animals
//! are WildAnimalComponents, an animal's herd a HerdComponent).

use banished_formats::savegame::{Fields, GameState, ObjId, Value};

use super::construction::like_saved;
use super::{component, fields_mut, Ids, Refs};
use crate::start::StartingTown;

fn ptr_list(name: &'static str, ids: impl IntoIterator<Item = Option<ObjId>>) -> Value {
    Value::List(
        ids.into_iter()
            .map(|id| Fields(vec![(name, Value::Ptr(id))]))
            .collect(),
    )
}

impl StartingTown {
    pub(super) fn export_wildlife(&self, g: &mut GameState, refs: &mut Refs, ids: &Ids) {
        let entity = |p: usize| ids.entity(p);
        let comp = |g: &GameState, p: usize, t: &str| entity(p).and_then(|e| component(g, e, t));
        let w = &self.wildlife;
        for pop in &w.populations {
            let Some(f) = entity(pop.entity).and_then(|e| fields_mut(g, e, "PopulationComponent"))
            else {
                continue;
            };
            f.set_i32("population.width", pop.width);
            f.set_i32("population.herd_width", pop.herd_width);
            // Written `[0], [2], [1]`.
            f.set(
                "population.cells",
                Value::List(
                    pop.cells
                        .iter()
                        .map(|c| {
                            Fields(vec![
                                ("a", Value::I32(c[0])),
                                ("c", Value::I32(c[2])),
                                ("b", Value::I32(c[1])),
                            ])
                        })
                        .collect(),
                ),
            );
            // Its herd list (`+0x50`) stays as saved: empty in every sample, the herds point at
            // their population instead.
            f.set_time("population.clock", pop.clock);
            f.set_i32("population.cursor", pop.cursor);
        }
        for herd in &w.herds {
            let animals: Vec<Option<ObjId>> = herd
                .animals
                .iter()
                .map(|&a| comp(g, a, "WildAnimalComponent"))
                .collect();
            let population = w
                .populations
                .get(herd.population)
                .and_then(|p| comp(g, p.entity, "PopulationComponent"));
            let Some(f) = entity(herd.entity).and_then(|e| fields_mut(g, e, "HerdComponent"))
            else {
                continue;
            };
            let animals = like_saved(f.at("herd.animals"), ptr_list("animal", animals));
            f.set("herd.animals", animals);
            f.set_bool("herd.moving", herd.moving);
            f.set_time("herd.rest", herd.rest);
            f.set_time("herd.regrow", herd.regrow);
            f.set_i32("herd.target", herd.target);
            f.set("herd.population", Value::Ptr(population));
            f.set_i32("herd.index", herd.index);
        }
        for (&a, animal) in &w.animals {
            let Some(e) = entity(a) else { continue };
            let herd = animal
                .herd
                .and_then(|h| w.herds.get(h))
                .and_then(|h| comp(g, h.entity, "HerdComponent"));
            let pasture = animal
                .livestock
                .and_then(|l| l.pasture)
                .and_then(|p| comp(g, p, "PastureComponent"));
            if let Some(f) = fields_mut(g, e, "AnimalComponent") {
                f.set_time("animal.timer", animal.timer);
                f.set_i32("animal.home", animal.home);
                f.set_f32("animal.age", animal.age);
                f.set_f32("animal.life", animal.life);
                f.set_bool("animal.male", animal.male);
                f.set_time("animal.breed", animal.breed);
                for (k, v) in ["x", "y", "w", "h"].iter().zip(animal.area) {
                    f.set_i32(&format!("animal.{k}"), v as i32);
                }
                // The animal's state bits over the component's capability bits.
                let bits = f.i32("base.flags").unwrap_or(0) as u32 & 0xffff;
                f.set_i32("base.flags", (animal.flags & !0xffff | bits) as i32);
            }
            if let Some(f) = fields_mut(g, e, "WildAnimalComponent") {
                f.set("wild.herd", Value::Ptr(herd));
            }
            if let (Some(l), Some(f)) = (animal.livestock, fields_mut(g, e, "LivestockComponent")) {
                f.set("livestock.pasture", Value::Ptr(pasture));
                f.set_time("livestock.timer", l.timer);
            }
            self.export_body(
                g,
                refs,
                ids,
                e,
                &animal.character,
                animal.variant,
                animal.mesh,
            );
        }
        for (&p, pasture) in &w.pastures {
            let kind = pasture
                .kind
                .map(|t| refs.id(g, &self.templates[t].reference));
            let animals: Vec<Option<ObjId>> = pasture
                .livestock
                .iter()
                .map(|&a| comp(g, a, "LivestockComponent"))
                .collect();
            let Some(f) = entity(p).and_then(|e| fields_mut(g, e, "PastureComponent")) else {
                continue;
            };
            f.set_i32("pasture.kind", kind.unwrap_or(u32::MAX) as i32);
            let animals = like_saved(f.at("pasture.animals"), ptr_list("animal", animals));
            f.set("pasture.animals", animals);
            f.set_i32("pasture.value_40", pasture.limit);
            f.set_i32("pasture.value_44", pasture.produced);
        }
        // The town's herds and populations (`+0xb0`, `+0xc0`).
        let herds: Vec<Option<ObjId>> = w
            .herds
            .iter()
            .map(|h| comp(g, h.entity, "HerdComponent"))
            .collect();
        let populations: Vec<Option<ObjId>> = w
            .populations
            .iter()
            .map(|p| comp(g, p.entity, "PopulationComponent"))
            .collect();
        let herds = like_saved(g.town.at("town.herds"), ptr_list("entity", herds));
        g.town.set("town.herds", herds);
        let populations = like_saved(
            g.town.at("town.populations"),
            ptr_list("entity", populations),
        );
        g.town.set("town.populations", populations);
        let pastures: Vec<Option<ObjId>> = w
            .pasture_list
            .iter()
            .map(|&p| comp(g, p, "PastureComponent"))
            .collect();
        let livestock: Vec<Option<ObjId>> = w
            .unassigned
            .iter()
            .map(|&a| comp(g, a, "LivestockComponent"))
            .collect();
        g.town.set("town.pastures", ptr_list("entity", pastures));
        g.town.set("town.livestock", ptr_list("entity", livestock));
        for (&b, boat) in &w.boats {
            let Some(e) = entity(b) else { continue };
            let post = boat.post.and_then(entity);
            if let Some(f) = fields_mut(g, e, "BoatComponent") {
                f.set("boat.post", Value::Ptr(post));
                f.set("boat.dock", Value::F32s(boat.dock.to_vec()));
                f.set_f32("boat.dock_yaw", boat.dock_yaw);
                f.set("boat.target", Value::F32s(boat.target.to_vec()));
                f.set_i32("boat.index", boat.index);
                f.set(
                    "boat.route",
                    Value::List(
                        boat.route
                            .iter()
                            .map(|p| Fields(vec![("point", Value::F32s(p.to_vec()))]))
                            .collect(),
                    ),
                );
            }
            if let Some(f) = fields_mut(g, e, "StateMachineComponent") {
                f.set_i32("machine.state", boat.state as i32);
            }
            self.export_body(g, refs, ids, e, &boat.character, 0, usize::MAX);
        }
    }

    /// A character's model (position, yaw and rotation, variant and mesh unless `mesh` is
    /// `usize::MAX`), walking state and stack.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn export_body(
        &self,
        g: &mut GameState,
        refs: &mut Refs,
        ids: &Ids,
        e: ObjId,
        ch: &banished_game::character::Character,
        variant: usize,
        mesh: usize,
    ) {
        if let Some(m) = fields_mut(g, e, "ModelComponent") {
            m.set("model.position", Value::F32s(ch.position.to_vec()));
            if m.f32("model.yaw") != Some(ch.yaw) {
                m.set_f32("model.yaw", ch.yaw);
                let q = banished_game::math::Transform::character_rotation(ch.yaw);
                m.set("model.rotation", Value::F32s(q.to_vec()));
            }
            if mesh != usize::MAX {
                m.set_i32("model.variant", variant as i32);
                m.set_i32("model.mesh", mesh as i32);
            }
        }
        if let Some(k) = fields_mut(g, e, "CharacterComponent") {
            k.set("character.direction", Value::F32s(ch.direction.to_vec()));
            k.set_f32("character.target_heading", ch.target_heading);
            k.set_f32("character.speed", ch.speed);
            k.set_bool("character.flag_28", ch.flag_28);
        }
        if let Some(character) = component(g, e, "CharacterComponent") {
            let owner = super::actions::Owner {
                entity: e,
                character,
                citizen: None,
            };
            self.export_stack(g, refs, ids, &owner, &ch.stack);
        }
    }
}

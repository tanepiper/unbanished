//! Saving, stage 3a (the inverse of `load::citizens`): births and deaths, each CitizenComponent
//! (`FUN_140119630`) field by its offset, the entity's model, character, AI, carried goods and
//! icons, the town's citizen list, the homes' families and the services' attendees.

use banished_formats::pkg::Archive;
use banished_formats::savegame::{Fields, GameState, ObjId, Value};

use super::create::{add_entity, new_entity, remove_entity};
use super::{component, fields_mut, Ids, Refs};
use crate::start::StartingTown;

/// The citizens' template (vanilla `Template\Citizen.rsc`).
const CITIZEN: &str = r"Template\Citizen.rsc:resource";
/// Its bucket (the citizens' scheduled update).
const CITIZEN_BUCKET: usize = 1;

/// A CitizenComponent's entity.
fn entity_of(g: &GameState, citizen: ObjId) -> Option<ObjId> {
    g.objects.get(&citizen)?.fields.ptr("base.entity")
}

impl StartingTown {
    /// Births get their entity (the citizen template's components); the dead leave the entity
    /// manager. Run before the other exports, which point at citizens.
    pub(super) fn sync_citizens(
        &self,
        archive: &Archive,
        g: &mut GameState,
        refs: &mut Refs,
        ids: &mut Ids,
    ) {
        ids.citizens.resize(self.citizens.len(), None);
        for (c, citizen) in self.citizens.iter().enumerate() {
            match ids.citizens[c] {
                Some(id) if !citizen.alive => {
                    if let Some(e) = entity_of(g, id) {
                        remove_entity(g, e);
                    }
                    ids.citizens[c] = None;
                }
                None if citizen.alive => {
                    let Some(e) = new_entity(archive, g, refs, CITIZEN) else {
                        continue;
                    };
                    add_entity(g, e, CITIZEN_BUCKET, self.scheduler.time);
                    ids.citizens[c] = component(g, e, "CitizenComponent");
                }
                _ => {}
            }
        }
    }

    pub(super) fn export_citizens(
        &self,
        archive: &Archive,
        g: &mut GameState,
        refs: &mut Refs,
        ids: &Ids,
    ) {
        // The disease list in the order `Agents::diseases` was read.
        let diseases: Vec<String> =
            crate::start::read(archive, "Game/Disease/Diseases.rsc:resource")
                .and_then(|d| banished_formats::game::external_list(&d).ok())
                .unwrap_or_default();
        let entity = |p: Option<usize>| p.and_then(|p| ids.entity(p));
        let citizen = |c: Option<usize>| c.and_then(|c| ids.citizen(c));
        let attendance = |g: &GameState, p: Option<usize>| {
            entity(p).and_then(|e| component(g, e, "AttendanceComponent"))
        };
        let professions = &self.construction.professions.list;
        for (c, cit) in self.citizens.iter().enumerate() {
            let Some(id) = ids.citizen(c) else { continue };
            let s = &self.agents.states[c];
            let profession = professions
                .get(s.profession)
                .map(|p| refs.id(g, &p.reference));
            let accident = s
                .accident
                .and_then(|a| professions.get(a))
                .map(|p| refs.id(g, &p.reference));
            let foods: Vec<u32> = s
                .foods
                .iter()
                .map(|&m| refs.id(g, &self.materials[m].reference))
                .collect();
            let disease = s
                .disease
                .and_then(|d| diseases.get(d))
                .map(|r| refs.id(g, r));
            let school = attendance(g, s.school);
            let doctor = attendance(g, s.doctor);
            let priest = attendance(g, s.priest);
            let ring_head = s.dependents.first().and_then(|&d| citizen(Some(d)));
            // The dependents' ring: each dependent's next and previous (`+0x268`, `+0x270`).
            let mut ring = Vec::new();
            for (k, &d) in s.dependents.iter().enumerate() {
                let n = s.dependents.len();
                let next = s.dependents[(k + 1) % n];
                let prev = s.dependents[(k + n - 1) % n];
                ring.push((d, citizen(Some(next)), citizen(Some(prev))));
            }
            let carried: Vec<Fields> = s
                .carried
                .items
                .iter()
                .map(|i| {
                    Fields(vec![
                        ("count", Value::I32(i.count)),
                        ("reserved", Value::I32(i.reserved)),
                        (
                            "material",
                            Value::Ext(refs.id(g, &self.materials[i.material].reference)),
                        ),
                    ])
                })
                .collect();
            let Some(o) = g.objects.get_mut(&id) else {
                continue;
            };
            let f = &mut o.fields;
            f.set("citizen.name", Value::Str(cit.name.clone()));
            f.set_i32("citizen.sex", cit.male as i32);
            f.set_f32("citizen.age", cit.age);
            f.set_f32("citizen.lifespan", cit.lifespan);
            f.set_i32("citizen.appearance", cit.appearance as i32);
            f.set_i32("citizen.education", cit.education);
            f.set_f32("citizen.hunger", s.hunger);
            f.set_f32("citizen.cold", s.cold);
            f.set_f32("citizen.reproduction", s.reproduction);
            f.set_f32("citizen.health", s.health);
            f.set_i32("citizen.health_events", s.health_events as i32);
            f.set_time("citizen.health_timer", s.health_timer);
            f.set_time("citizen.disease_clock", s.disease_clock);
            f.set_i32("citizen.disease", disease.unwrap_or(u32::MAX) as i32);
            // `+0x70`: the illness left while sick, the immunity left after.
            f.set_time(
                "citizen.disease_left",
                if s.disease.is_some() {
                    s.disease_left
                } else {
                    s.immunity
                },
            );
            f.set_time("citizen.spread", s.spread);
            f.set_f32("citizen.happiness", s.happiness);
            f.set_i32("citizen.flags", s.flags as i32);
            f.set_time("citizen.happiness_timer", s.happiness_timer);
            f.set_i32("citizen.food_count", foods.len() as i32);
            f.set("citizen.foods", Value::U32s(foods));
            if let Some(p) = profession {
                f.set_i32("citizen.profession", p as i32);
            }
            f.set_i32("citizen.tool_uses", s.tool_uses);
            f.set_i32("citizen.tool_status", s.tool_status);
            f.set_i32("citizen.clothing", s.clothing);
            f.set_time("citizen.clothing_remaining", s.clothing_remaining);
            f.set_i32("citizen.clothing_status", s.clothing_status);
            f.set_i32("citizen.supply_activity", s.supply_activity);
            f.set_i32("citizen.job", s.job);
            f.set("citizen.home", Value::Ptr(entity(s.home)));
            f.set_i32("citizen.home_slot", s.home_slot);
            f.set("citizen.spouse", Value::Ptr(citizen(cit.spouse)));
            f.set("citizen.guardian", Value::Ptr(citizen(cit.parent)));
            f.set("citizen.ring_head", Value::Ptr(ring_head));
            f.set("citizen.workplace", Value::Ptr(entity(s.workplace)));
            f.set_i32("citizen.accident", accident.unwrap_or(u32::MAX) as i32);
            f.set("citizen.school", Value::Ptr(school));
            f.set_time("citizen.school_time", s.school_time);
            f.set_bool("citizen.schooled", s.schooled);
            f.set("citizen.doctor", Value::Ptr(doctor));
            f.set("citizen.priest", Value::Ptr(priest));
            for (d, next, prev) in ring {
                if let Some(o) = ids.citizen(d).and_then(|d| g.objects.get_mut(&d)) {
                    o.fields.set("citizen.ring_next", Value::Ptr(next));
                    o.fields.set("citizen.ring_prev", Value::Ptr(prev));
                }
            }
            let Some(e) = entity_of(g, id) else { continue };
            let ch = &self.agents.characters[c];
            if let Some(m) = fields_mut(g, e, "ModelComponent") {
                m.set("model.position", Value::F32s(ch.position.to_vec()));
                if m.f32("model.yaw") != Some(ch.yaw) {
                    m.set_f32("model.yaw", ch.yaw);
                    let half = ch.yaw.to_radians() / 2.0;
                    m.set(
                        "model.rotation",
                        Value::F32s(vec![-0.0, -0.0, -half.sin(), half.cos()]),
                    );
                }
            }
            if let Some(k) = fields_mut(g, e, "CharacterComponent") {
                k.set("character.direction", Value::F32s(ch.direction.to_vec()));
                k.set_f32("character.target_heading", ch.target_heading);
                k.set_f32("character.speed", ch.speed);
                k.set_bool("character.flag_28", ch.flag_28);
            }
            let ai = &self.agents.ais[c];
            if let Some(a) = fields_mut(g, e, "CitizenAIComponent") {
                a.set_i32("ai.current_activity", ai.current as i32);
                a.set_i32("ai.queued_activity", ai.queued.map_or(-1, |q| q as i32));
                if let Some(Value::I64s(t)) = a.at_mut("ai.cooldowns") {
                    for (dst, src) in t.iter_mut().zip(&ai.cooldowns) {
                        *dst = *src;
                    }
                }
                a.set_bool("ai.urgent_eating", ai.eating);
                a.set_bool("ai.urgent_warming", ai.warming);
                a.set_time("ai.since_loiter", ai.since_loiter);
                a.set_time("ai.carrying_time", ai.carrying_time);
            }
            if let Some(st) = fields_mut(g, e, "StorageComponent") {
                st.set("storage.items", Value::List(carried));
                st.set_i32("storage.capacity", s.carried.capacity);
                st.set_i32("storage.accepts", s.carried.accepts as i32);
            }
            if let Some(icon) = fields_mut(g, e, "StatusIconComponent") {
                icon.set_i32("status.bits", s.status as i32);
            }
            if let Some(character) = component(g, e, "CharacterComponent") {
                let owner = super::actions::Owner {
                    entity: e,
                    character,
                    citizen: Some(id),
                };
                self.export_stack(g, refs, ids, &owner, &ch.stack);
            }
        }
        // The town's citizens, in activation order (`+0xa0`).
        let list = self
            .agents
            .registry
            .iter()
            .filter(|&&c| self.citizens.get(c).is_some_and(|c| c.alive))
            .map(|&c| Fields(vec![("entity", Value::Ptr(ids.citizen(c)))]))
            .collect();
        g.town.set("town.citizens", Value::List(list));
        // Homes' families, and the services' attendees.
        let c = &self.construction;
        let homes = c.homes.iter().map(|(&e, h)| (e, h)).chain(
            c.sites
                .iter()
                .filter_map(|(&e, s)| Some((e, s.home.as_ref()?))),
        );
        let mut families = Vec::new();
        for (e, h) in homes {
            let list: Vec<Fields> = h
                .families
                .iter()
                .map(|f| {
                    Fields(vec![(
                        "members",
                        Value::List(
                            f.iter()
                                .map(|&m| Fields(vec![("citizen", Value::Ptr(ids.citizen(m)))]))
                                .collect(),
                        ),
                    )])
                })
                .collect();
            families.push((e, list, h.fuel));
        }
        for (e, list, fuel) in families {
            if let Some(f) = ids
                .entity(e)
                .and_then(|id| fields_mut(g, id, "ResidenceComponent"))
            {
                f.set("residence.families", Value::List(list));
                f.set_time("residence.fuel", fuel);
            }
        }
        for (&e, a) in &c.attendance {
            let list = Value::List(
                a.attendees
                    .iter()
                    .map(|&m| Fields(vec![("citizen", Value::Ptr(ids.citizen(m)))]))
                    .collect(),
            );
            if let Some(f) = ids
                .entity(e)
                .and_then(|id| fields_mut(g, id, "AttendanceComponent"))
            {
                f.set("attendance.visitors", list);
            }
        }
    }
}

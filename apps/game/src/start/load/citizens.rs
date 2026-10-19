//! Stage 3a of loading a save (docs/SAVE-LOADING.md): the citizens. Each CitizenComponent
//! (`FUN_140119630`) gives the citizen and its state; its entity's ModelComponent the body's
//! place, CharacterComponent the walking state, CitizenAIComponent (`FUN_140114e30`) the AI,
//! StorageComponent the goods carried and StatusIconComponent the icons. The homes' families,
//! workplaces' workers and services' attendees follow.
//!
//! Action stacks are not restored yet (stage 3b): every citizen starts with an empty stack and
//! chooses a new activity on its first update, as one whose actions all finished would.

use std::collections::BTreeMap;

use banished_formats::pkg::Archive;
use banished_formats::savegame::{Fields, ObjId, Object, Value};
use banished_game::people::citizens::{Citizen, CitizenRules};
use banished_game::work::stock::{Item, Storage};

use super::super::{material_index, normalise, StartingTown};
use crate::saves::saved::SavedGame;

/// Citizen flag `0x400`: educated (`FUN_140116c70`).
const EDUCATED: u32 = 0x400;

fn time(f: &Fields, path: &str) -> i64 {
    match f.at(path) {
        Some(Value::I64(t)) => *t,
        _ => 0,
    }
}

fn flag(f: &Fields, path: &str) -> bool {
    match f.at(path) {
        Some(Value::Bool(b)) => *b,
        Some(Value::I8(b)) => *b != 0,
        _ => false,
    }
}

fn int(f: &Fields, path: &str) -> i32 {
    f.i32(path).unwrap_or(0)
}

fn list<'a>(f: &'a Fields, path: &str) -> &'a [Fields] {
    match f.at(path) {
        Some(Value::List(items)) => items,
        _ => &[],
    }
}

fn floats(f: &Fields, path: &str) -> Vec<f32> {
    match f.at(path) {
        Some(Value::F32s(v)) => v.clone(),
        _ => Vec::new(),
    }
}

/// The saved citizens in the town manager's registry order (`+0xa0`), and each
/// CitizenComponent's index among them.
pub(in crate::start) fn saved_citizens(
    saved: &SavedGame,
    rules: &CitizenRules,
) -> (Vec<Citizen>, BTreeMap<ObjId, usize>) {
    let components: Vec<(ObjId, &Object)> = list(&saved.state.town, "town.citizens")
        .iter()
        .filter_map(|c| {
            let id = c.ptr("entity")?;
            Some((id, saved.object(id)?))
        })
        .collect();
    let order: BTreeMap<ObjId, usize> = components
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let citizen = |f: &Fields, name: &str| {
        f.ptr(&format!("citizen.{name}"))
            .and_then(|c| order.get(&c).copied())
    };
    let citizens = components
        .iter()
        .map(|(_, c)| {
            let f = &c.fields;
            let position = c
                .fields
                .ptr("base.entity")
                .and_then(|e| saved.object(e))
                .and_then(|e| saved.component(e, "ModelComponent"))
                .map(|m| floats(&m.fields, "model.position"))
                .and_then(|p| <[f32; 3]>::try_from(p).ok())
                .unwrap_or([0.0; 3]);
            let education = int(f, "citizen.education");
            Citizen {
                name: match f.get("citizen") {
                    Some(Value::Block(_, b)) => match b.get("name") {
                        Some(Value::Str(s)) => s.clone(),
                        _ => String::new(),
                    },
                    _ => String::new(),
                },
                male: int(f, "citizen.sex") == 1,
                age: f.f32("citizen.age").unwrap_or(0.0),
                lifespan: f.f32("citizen.lifespan").unwrap_or(0.0),
                appearance: int(f, "citizen.appearance") as u32,
                // Inferred: flag 0x400, or the education an educated starting adult gets.
                educated: int(f, "citizen.flags") as u32 & EDUCATED != 0
                    || (rules.education > 0 && education >= rules.education),
                education,
                position,
                spouse: citizen(f, "spouse"),
                parent: citizen(f, "guardian"),
                school: None,
                alive: true,
            }
        })
        .collect();
    (citizens, order)
}

impl StartingTown {
    /// The citizens' state, after the buildings (homes, workplaces, services) are restored.
    pub(in crate::start) fn restore_citizens(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
        order: &BTreeMap<ObjId, usize>,
    ) {
        let placed_of = |id: Option<ObjId>| id.and_then(|i| index.get(&i).copied());
        // A component's entity, as a placed index (AttendanceComponent pointers).
        let entity_of = |id: Option<ObjId>| {
            id.and_then(|i| saved.object(i))
                .and_then(|o| o.fields.ptr("base.entity"))
                .and_then(|e| index.get(&e).copied())
        };
        let citizen_of = |id: Option<ObjId>| id.and_then(|i| order.get(&i).copied());
        let path_of = |id: u32| saved.reference(id).map(normalise);
        let profession_index = |town: &Self, path: &str| {
            let key = normalise(path);
            town.construction
                .professions
                .list
                .iter()
                .position(|p| normalise(&p.reference) == key)
        };
        // The disease list in the order `Agents::diseases` was read (`Game/Disease/Diseases.rsc`).
        let diseases: Vec<String> =
            super::super::read(archive, "Game/Disease/Diseases.rsc:resource")
                .and_then(|d| banished_formats::game::external_list(&d).ok())
                .unwrap_or_default()
                .iter()
                .map(|r| normalise(r))
                .collect();
        for (&id, &c) in order {
            let Some(o) = saved.object(id) else { continue };
            let f = &o.fields;
            let entity = f.ptr("base.entity").and_then(|e| saved.object(e));
            let component = |name: &str| entity.and_then(|e| saved.component(e, name));
            let foods: Vec<usize> = match f.at("citizen.foods") {
                Some(Value::U32s(v)) => v
                    .iter()
                    .filter_map(|&m| material_index(archive, &mut self.materials, &path_of(m)?))
                    .collect(),
                _ => Vec::new(),
            };
            let profession = saved
                .ext(f, "citizen.profession")
                .and_then(|p| profession_index(self, p))
                .unwrap_or(self.construction.professions.laborer);
            let accident = saved
                .ext(f, "citizen.accident")
                .and_then(|p| profession_index(self, p));
            let disease = saved
                .ext(f, "citizen.disease")
                .and_then(|d| diseases.iter().position(|x| *x == normalise(d)));
            let carried = component("StorageComponent").map(|s| {
                let items = list(&s.fields, "storage.items")
                    .iter()
                    .filter_map(|i| {
                        Some(Item {
                            material: material_index(
                                archive,
                                &mut self.materials,
                                &normalise(saved.ext(i, "material")?),
                            )?,
                            count: int(i, "count"),
                            reserved: int(i, "reserved"),
                        })
                    })
                    .collect();
                Storage {
                    capacity: int(&s.fields, "storage.capacity"),
                    accepts: int(&s.fields, "storage.accepts") as u32,
                    items,
                }
            });
            // The dependents' ring (`+0x260` head, `+0x268` next), in ring order.
            let mut dependents = Vec::new();
            if let Some(head) = f.ptr("citizen.ring_head") {
                let mut at = head;
                while let Some(i) = order.get(&at).copied() {
                    dependents.push(i);
                    match saved
                        .object(at)
                        .and_then(|d| d.fields.ptr("citizen.ring_next"))
                    {
                        Some(next) if next != head && dependents.len() < order.len() => at = next,
                        _ => break,
                    }
                }
            }
            let school = entity_of(f.ptr("citizen.school"));
            self.citizens[c].school = school;
            let s = &mut self.agents.states[c];
            s.hunger = f.f32("citizen.hunger").unwrap_or(0.0);
            s.cold = f.f32("citizen.cold").unwrap_or(0.0);
            s.reproduction = f.f32("citizen.reproduction").unwrap_or(0.0);
            s.health = f.f32("citizen.health").unwrap_or(s.health);
            s.health_events = int(f, "citizen.health_events") as u32;
            s.health_timer = time(f, "citizen.health_timer");
            s.disease_clock = time(f, "citizen.disease_clock");
            s.disease = disease;
            // `+0x70`: the illness left while sick, the immunity left after.
            if disease.is_some() {
                s.sick = true;
                s.disease_left = time(f, "citizen.disease_left");
            } else {
                s.immunity = time(f, "citizen.disease_left");
            }
            s.spread = time(f, "citizen.spread");
            s.happiness = f.f32("citizen.happiness").unwrap_or(s.happiness);
            s.flags = int(f, "citizen.flags") as u32;
            s.happiness_timer = time(f, "citizen.happiness_timer");
            s.foods = foods;
            s.profession = profession;
            s.tool_uses = int(f, "citizen.tool_uses");
            s.tool_status = int(f, "citizen.tool_status");
            s.clothing = int(f, "citizen.clothing");
            s.clothing_remaining = time(f, "citizen.clothing_remaining");
            s.clothing_status = int(f, "citizen.clothing_status");
            s.supply_activity = int(f, "citizen.supply_activity");
            s.job = int(f, "citizen.job");
            s.home = placed_of(f.ptr("citizen.home"));
            s.home_slot = int(f, "citizen.home_slot");
            s.workplace = placed_of(f.ptr("citizen.workplace"));
            s.accident = accident;
            s.school = school;
            s.school_time = time(f, "citizen.school_time");
            s.schooled = flag(f, "citizen.schooled");
            s.doctor = entity_of(f.ptr("citizen.doctor"));
            s.priest = entity_of(f.ptr("citizen.priest"));
            s.dependents = dependents;
            if let Some(carried) = carried {
                s.carried = carried;
            }
            if let Some(icon) = component("StatusIconComponent") {
                s.status = int(&icon.fields, "status.bits") as u32;
            }
            let ch = &mut self.agents.characters[c];
            if let Some(m) = component("ModelComponent") {
                if let Ok(p) = <[f32; 3]>::try_from(floats(&m.fields, "model.position")) {
                    ch.position = p;
                }
                ch.yaw = m.fields.f32("model.yaw").unwrap_or(ch.yaw);
            }
            if let Some(k) = component("CharacterComponent") {
                if let Ok(d) = <[f32; 3]>::try_from(floats(&k.fields, "character.direction")) {
                    ch.direction = d;
                }
                ch.target_heading = k.fields.f32("character.target_heading").unwrap_or(0.0);
                ch.speed = k.fields.f32("character.speed").unwrap_or(0.0);
                ch.flag_28 = flag(&k.fields, "character.flag_28");
            }
            if let Some(a) = component("CitizenAIComponent") {
                let a = &a.fields;
                let ai = &mut self.agents.ais[c];
                ai.current = int(a, "ai.current_activity") as u32;
                let queued = int(a, "ai.queued_activity");
                ai.queued = (queued >= 0).then_some(queued as u32);
                if let Some(Value::I64s(t)) = a.at("ai.cooldowns") {
                    for (dst, src) in ai.cooldowns.iter_mut().zip(t) {
                        *dst = *src;
                    }
                }
                ai.eating = flag(a, "ai.urgent_eating");
                ai.warming = flag(a, "ai.urgent_warming");
                ai.since_loiter = time(a, "ai.since_loiter");
                ai.carrying_time = time(a, "ai.carrying_time");
            }
        }
        self.agents.registry = (0..self.citizens.len()).collect();

        // Homes, workplaces' workers and services' attendees.
        for (id, entity) in saved.entities() {
            let Some(&e) = index.get(&id) else { continue };
            if let Some(r) = saved.component(entity, "ResidenceComponent") {
                let families: Vec<Vec<usize>> = list(&r.fields, "residence.families")
                    .iter()
                    .map(|fam| {
                        list(fam, "members")
                            .iter()
                            .filter_map(|m| citizen_of(m.ptr("citizen")))
                            .collect()
                    })
                    .collect();
                let fuel = time(&r.fields, "residence.fuel");
                let registered = self.construction.workforce.residences.contains(&e);
                if let Some(site) = self.construction.sites.get_mut(&e) {
                    if let Some(desc) = self.construction.templates[site.template].residence {
                        let mut home = banished_game::people::life::Home::new(desc, site.storage);
                        if !families.is_empty() {
                            home.families = families;
                        }
                        home.fuel = fuel;
                        // Registered homes are the town's (`ResidenceEnter2`); one out of the
                        // town's (states 3 to 7) keeps its component on the site.
                        if registered {
                            self.construction.homes.insert(e, home);
                        } else {
                            site.home = Some(home);
                        }
                    }
                }
            }
            if let Some(w) = saved.component(entity, "WorkPlaceComponent") {
                let workers: Vec<usize> = list(&w.fields, "workplace.workers")
                    .iter()
                    .filter_map(|m| citizen_of(m.ptr("worker")))
                    .collect();
                if let Some(wp) = self.construction.workforce.workplaces.get_mut(&e) {
                    wp.workers = workers;
                }
            }
            if let Some(a) = saved.component(entity, "AttendanceComponent") {
                let attendees: Vec<usize> = list(&a.fields, "attendance.visitors")
                    .iter()
                    .filter_map(|m| citizen_of(m.ptr("citizen")))
                    .collect();
                let site = &self.construction.sites[&e];
                let capacity = self.construction.templates[site.template]
                    .attendance
                    .unwrap_or(15);
                let profession = self
                    .construction
                    .workforce
                    .workplaces
                    .get(&e)
                    .and_then(|w| w.profession);
                self.construction.attendance.insert(
                    e,
                    banished_game::work::construction::Attendance {
                        capacity,
                        attendees,
                        profession,
                    },
                );
            }
        }
        let attendance: Vec<usize> = list(&saved.state.town, "town.attendance")
            .iter()
            .filter_map(|a| entity_of(a.ptr("entity")))
            .collect();
        self.construction.attendance_list = attendance;
    }
}

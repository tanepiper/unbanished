//! Stage 3c of loading a save (docs/SAVE-LOADING.md): wildlife, livestock and trader boats.
//!
//! The populations (`FUN_140054e00`), herds (`FUN_140127460`) and animals (AnimalComponent
//! `FUN_14010c310`, WildAnimalComponent `FUN_1401337d0`, LivestockComponent `FUN_1401292e0`)
//! become the port's `Wildlife` entries; their kinds are loaded from the templates as a New
//! Game does. The pastures' livestock (`FUN_14012d410`), the town's pasture list (`+0x90`) and
//! its livestock without a pasture (`+0x80`) follow. Trader boats (BoatComponent
//! `FUN_140046f30`) keep their state, post, dock, route and place on it.

use std::collections::BTreeMap;

use banished_formats::pkg::Archive;
use banished_formats::savegame::{Fields, ObjId, Value};
use banished_game::behaviour::character::Character;
use banished_game::nature::wildlife::{Animal, Herd, Livestock, Population};

use super::super::{livestock_kind, normalise, wildlife_kinds, StartingTown};
use super::construction::entity_or_component;
use crate::saved::SavedGame;

fn int(f: &Fields, path: &str) -> i32 {
    f.i32(path).unwrap_or(0)
}

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

fn list<'a>(f: &'a Fields, path: &str) -> &'a [Fields] {
    match f.at(path) {
        Some(Value::List(items)) => items,
        _ => &[],
    }
}

fn vec3(f: &Fields, path: &str) -> Option<[f32; 3]> {
    match f.at(path) {
        Some(Value::F32s(v)) if v.len() == 3 => Some([v[0], v[1], v[2]]),
        _ => None,
    }
}

impl StartingTown {
    pub(in crate::start) fn restore_wildlife(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
    ) {
        let template_ref = |e: &banished_formats::savegame::Object| {
            saved.ext(&e.fields, "entity.template").map(normalise)
        };
        // The kinds, as a New Game loads them for its populations and livestock.
        let mut populations = Vec::new();
        let mut livestock = Vec::new();
        let mut boats = Vec::new();
        for (_, e) in saved.entities() {
            let Some(r) = template_ref(e) else { continue };
            if saved.component(e, "PopulationComponent").is_some() && !populations.contains(&r) {
                populations.push(r);
            } else if saved.component(e, "LivestockComponent").is_some() && !livestock.contains(&r)
            {
                livestock.push(r);
            } else if saved.component(e, "BoatComponent").is_some() && !boats.contains(&r) {
                boats.push(r);
            }
        }
        wildlife_kinds(
            archive,
            &populations,
            &mut self.templates,
            &mut self.wildlife,
        );
        for r in &livestock {
            livestock_kind(archive, r, &mut self.templates, &mut self.wildlife);
        }
        for r in &boats {
            self.load_boat(archive, r);
        }

        // Populations, then herds (they point at their population), then animals.
        let mut population_index: BTreeMap<ObjId, usize> = BTreeMap::new();
        for (id, e) in saved.entities() {
            let Some(&entity) = index.get(&id) else {
                continue;
            };
            let Some(c) = saved.component(e, "PopulationComponent") else {
                continue;
            };
            let f = &c.fields;
            let p = self.wildlife.restore_population(Population {
                entity,
                template: self.placed[entity].template,
                width: int(f, "population.width"),
                // Written `[0], [2], [1]`.
                cells: list(f, "population.cells")
                    .iter()
                    .map(|c| [int(c, "a"), int(c, "b"), int(c, "c")])
                    .collect(),
                herd_width: int(f, "population.herd_width"),
                clock: time(f, "population.clock"),
                cursor: int(f, "population.cursor"),
            });
            population_index.insert(c.index, p);
        }
        let mut herd_index: BTreeMap<ObjId, usize> = BTreeMap::new();
        for (id, e) in saved.entities() {
            let Some(&entity) = index.get(&id) else {
                continue;
            };
            let Some(c) = saved.component(e, "HerdComponent") else {
                continue;
            };
            let f = &c.fields;
            let h = self.wildlife.restore_herd(Herd {
                entity,
                template: self.placed[entity].template,
                population: f
                    .ptr("herd.population")
                    .and_then(|p| population_index.get(&p).copied())
                    .unwrap_or(0),
                index: int(f, "herd.index"),
                animals: list(f, "herd.animals")
                    .iter()
                    .filter_map(|a| entity_or_component(saved, index, a.ptr("animal")))
                    .collect(),
                moving: flag(f, "herd.moving"),
                target: int(f, "herd.target"),
                rest: time(f, "herd.rest"),
                regrow: time(f, "herd.regrow"),
            });
            herd_index.insert(c.index, h);
        }
        let mut dropped: BTreeMap<String, usize> = BTreeMap::new();
        for (id, e) in saved.entities() {
            let Some(&entity) = index.get(&id) else {
                continue;
            };
            let Some(c) = saved.component(e, "AnimalComponent") else {
                continue;
            };
            let template = self.placed[entity].template;
            let Some(kind) = self.wildlife.animal_kinds.get(&template).cloned() else {
                *dropped
                    .entry(self.templates[template].reference.clone())
                    .or_default() += 1;
                continue;
            };
            let f = &c.fields;
            let model = saved.component(e, "ModelComponent");
            let position = model
                .and_then(|m| vec3(&m.fields, "model.position"))
                .unwrap_or(self.placed[entity].position);
            let age = f.f32("animal.age").unwrap_or(0.0);
            // `FUN_14010c7f0`: a third of the size, full by the adult age.
            let scale = (age / kind.desc.adult * 0.67 + 0.33).min(1.0);
            let mut character = Character::new(kind.character, position, scale);
            if let Some(m) = model {
                character.yaw = m.fields.f32("model.yaw").unwrap_or(character.yaw);
            }
            if let Some(k) = saved.component(e, "CharacterComponent") {
                if let Some(d) = vec3(&k.fields, "character.direction") {
                    character.direction = d;
                }
                character.target_heading = k.fields.f32("character.target_heading").unwrap_or(0.0);
                character.speed = k.fields.f32("character.speed").unwrap_or(0.0);
                character.flag_28 = flag(&k.fields, "character.flag_28");
                match self.stack(archive, saved, index, k) {
                    Ok(stack) => character.stack = stack,
                    Err(kind) => *dropped.entry(format!("stack: {kind}")).or_default() += 1,
                }
            }
            let model_i = |name: &str| {
                model
                    .and_then(|m| m.fields.i32(&format!("model.{name}")))
                    .unwrap_or(0)
                    .max(0) as usize
            };
            let area = ["x", "y", "w", "h"].map(|k| int(f, &format!("animal.{k}")) as i16);
            let herd = saved
                .component(e, "WildAnimalComponent")
                .and_then(|w| w.fields.ptr("wild.herd"))
                .and_then(|h| herd_index.get(&h).copied());
            let livestock = saved.component(e, "LivestockComponent").map(|l| Livestock {
                pasture: entity_or_component(saved, index, l.fields.ptr("livestock.pasture")),
                timer: time(&l.fields, "livestock.timer"),
            });
            self.wildlife.restore_animal(
                entity,
                Animal {
                    template,
                    character,
                    herd,
                    flags: int(f, "base.flags") as u32,
                    home: int(f, "animal.home"),
                    timer: time(f, "animal.timer"),
                    area,
                    male: flag(f, "animal.male"),
                    age,
                    life: f.f32("animal.life").unwrap_or(0.0),
                    breed: time(f, "animal.breed"),
                    livestock,
                    variant: model_i("variant"),
                    mesh: model_i("mesh"),
                    scale,
                    dying: false,
                },
            );
        }

        // Pastures: the kind kept, the animals, the limit and the products gathered.
        for (id, e) in saved.entities() {
            let Some(&entity) = index.get(&id) else {
                continue;
            };
            let Some(c) = saved.component(e, "PastureComponent") else {
                continue;
            };
            let f = &c.fields;
            let kind = saved.ext(f, "pasture.kind").and_then(|r| {
                super::super::load_template(archive, &normalise(r), &mut self.templates)
            });
            let animals: Vec<usize> = list(f, "pasture.animals")
                .iter()
                .filter_map(|a| entity_or_component(saved, index, a.ptr("animal")))
                .collect();
            if let Some(p) = self.wildlife.pastures.get_mut(&entity) {
                p.kind = kind;
                p.livestock = animals;
                p.limit = int(f, "pasture.value_40");
                p.produced = int(f, "pasture.value_44");
            }
        }
        let town_list = |name: &str| -> Vec<usize> {
            list(&saved.state.town, &format!("town.{name}"))
                .iter()
                .filter_map(|e| entity_or_component(saved, index, e.ptr("entity")))
                .collect()
        };
        self.wildlife.pasture_list = town_list("pastures");
        self.wildlife.unassigned = town_list("livestock");

        // Trader boats.
        for (id, e) in saved.entities() {
            let Some(&entity) = index.get(&id) else {
                continue;
            };
            let Some(c) = saved.component(e, "BoatComponent") else {
                continue;
            };
            let template = self.placed[entity].template;
            let Some(kind) = self.wildlife.boat_kinds.get(&template).copied() else {
                continue;
            };
            let f = &c.fields;
            let position = saved
                .component(e, "ModelComponent")
                .and_then(|m| vec3(&m.fields, "model.position"))
                .unwrap_or(self.placed[entity].position);
            let mut character = Character::new(kind.character, position, 1.0);
            if let Some(m) = saved.component(e, "ModelComponent") {
                character.yaw = m.fields.f32("model.yaw").unwrap_or(character.yaw);
            }
            if let Some(k) = saved.component(e, "CharacterComponent") {
                if let Some(d) = vec3(&k.fields, "character.direction") {
                    character.direction = d;
                }
                character.target_heading = k.fields.f32("character.target_heading").unwrap_or(0.0);
                character.speed = k.fields.f32("character.speed").unwrap_or(0.0);
                character.flag_28 = flag(&k.fields, "character.flag_28");
                if let Ok(stack) = self.stack(archive, saved, index, k) {
                    character.stack = stack;
                }
            }
            let state = saved
                .component(e, "StateMachineComponent")
                .map_or(0, |m| int(&m.fields, "machine.state") as i8);
            self.wildlife.boats.insert(
                entity,
                banished_game::commerce::boat::Boat {
                    template,
                    character,
                    state,
                    post: entity_or_component(saved, index, f.ptr("boat.post")),
                    dock: vec3(f, "boat.dock").unwrap_or([0.0; 3]),
                    dock_yaw: f.f32("boat.dock_yaw").unwrap_or(0.0),
                    target: vec3(f, "boat.target").unwrap_or(position),
                    route: list(f, "boat.route")
                        .iter()
                        .filter_map(|p| vec3(p, "point"))
                        .collect(),
                    index: int(f, "boat.index"),
                },
            );
        }
        log::info!(
            "wildlife: {} populations, {} herds, {} animals, {} boats",
            self.wildlife.populations.len(),
            self.wildlife.herds.len(),
            self.wildlife.animals.len(),
            self.wildlife.boats.len()
        );
        if !dropped.is_empty() {
            log::warn!("wildlife not restored: {dropped:?}");
        }
    }
}

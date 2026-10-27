//! Stage 3b of loading a save (docs/SAVE-LOADING.md): each citizen's action stack
//! (CharacterComponent, top, "on empty"). Every saved entry is one action and its started flag;
//! actions an entry linked are not saved.
//!
//! A citizen whose stack holds an action the port cannot rebuild (one aimed at an animal, or at a
//! storage it does not know) starts with an empty stack instead, as one whose actions all finished
//! would; the load log counts them.
//!
//! Original: `FUN_14010ef80`.
// +0x18; +0x10; +0x48; +0x60; +0x58

use std::collections::BTreeMap;

use unbanished_formats::crc::type_id;
use unbanished_formats::pkg::Archive;
use unbanished_formats::savegame::{Fields, ObjId, Object, Value};
use unbanished_game::behaviour::character::{Action, ActionStack, Component, Message, MoveTo};

use super::super::{load_template, material_index, normalise, StartingTown};
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

/// SendComponentMessage's component type (`+0x30`) as the port's message target.
fn component(ty: u32) -> Option<Component> {
    let names: [(&str, Component); 11] = [
        ("CitizenAIComponent", Component::CitizenAi),
        ("CitizenComponent", Component::Citizen),
        ("HappinessComponent", Component::Happiness),
        ("BuildComponent", Component::Build),
        ("DestroyComponent", Component::Destroy),
        ("FireComponent", Component::Fire),
        ("CropFieldComponent", Component::CropField),
        ("OrchardComponent", Component::Orchard),
        ("PastureComponent", Component::Pasture),
        ("QuarryComponent", Component::Quarry),
        ("FishingComponent", Component::Fishing),
    ];
    names
        .iter()
        .find(|(n, _)| type_id(n) == ty)
        .map(|(_, c)| *c)
}

impl StartingTown {
    /// Every citizen's stack, after its citizen, the sites, the works and the animals are
    /// restored.
    pub(in crate::town) fn restore_actions(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
        order: &BTreeMap<ObjId, usize>,
    ) {
        let mut dropped: BTreeMap<String, usize> = BTreeMap::new();
        let mut restored = 0;
        for (&id, &c) in order {
            let Some(character) = saved
                .object(id)
                .and_then(|o| o.fields.ptr("base.entity"))
                .and_then(|e| saved.object(e))
                .and_then(|e| saved.component(e, "CharacterComponent"))
            else {
                continue;
            };
            match self.stack(archive, saved, index, character) {
                Ok(stack) => {
                    restored += 1;
                    self.agents.characters[c].stack = stack;
                }
                Err(kind) => *dropped.entry(kind).or_default() += 1,
            }
        }
        log::info!("restored {restored} action stacks");
        if !dropped.is_empty() {
            log::warn!("action stacks started empty, by the action not rebuilt: {dropped:?}");
        }
    }

    /// A CharacterComponent's stack, or the type of the first action the port cannot rebuild.
    pub(in crate::town) fn stack(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
        character: &Object,
    ) -> Result<ActionStack, String> {
        let f = &character.fields;
        let owner = f.ptr("base.entity");
        let top = int(f, "character.stack_top");
        let mut entries = Vec::new();
        for slot in list(f, "character.stack")
            .iter()
            .take((top + 1).max(0) as usize)
        {
            let Some(a) = slot.ptr("action").and_then(|a| saved.object(a)) else {
                continue;
            };
            let action = self
                .action(archive, saved, index, owner, a)
                .ok_or_else(|| a.type_name.to_string())?;
            entries.push((action, flag(&a.fields, "action.started")));
        }
        let on_empty = match f.ptr("character.on_empty").and_then(|a| saved.object(a)) {
            Some(a) => Some(
                self.action(archive, saved, index, owner, a)
                    .ok_or_else(|| a.type_name.to_string())?,
            ),
            None => None,
        };
        Ok(ActionStack::restored(entries, on_empty))
    }

    /// One saved action as the port's, or `None` when the port cannot rebuild it.
    fn action(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
        owner: Option<ObjId>,
        a: &Object,
    ) -> Option<Action> {
        let f = &a.fields;
        let placed = |path: &str| f.ptr(path).and_then(|e| index.get(&e).copied());
        // A storage entity's StorageComponent (index into the town's storages).
        let storage = |town: &Self, path: &str| {
            placed(path).and_then(|e| town.construction.sites.get(&e).map(|s| s.storage))
        };
        let items = |town: &mut Self, path: &str| -> Option<Vec<(usize, i32)>> {
            list(f, path)
                .iter()
                .map(|i| {
                    let m = material_index(
                        archive,
                        &mut town.materials,
                        &normalise(saved.ext(i, "material")?),
                    )?;
                    Some((m, int(i, "count")))
                })
                .collect()
        };
        Some(match a.type_name {
            "MoveTo" => {
                let target = match f.at("move.target") {
                    Some(Value::F32s(v)) if v.len() == 3 => [v[0], v[1], v[2]],
                    _ => return None,
                };
                let path: Vec<i32> = match f.at("move.path") {
                    Some(Value::U32s(v)) => v.iter().map(|&c| c as i32).collect(),
                    _ => Vec::new(),
                };
                Action::move_to(MoveTo::restored(
                    target,
                    int(f, "move.walk_clip").max(0) as usize,
                    int(f, "move.stand_clip").max(0) as usize,
                    int(f, "move.flags") as u32,
                    int(f, "move.cost_mode") as u32,
                    f.f32("move.speed_factor").unwrap_or(1.0),
                    flag(f, "move.following"),
                    int(f, "move.path_index"),
                    &path,
                ))
            }
            "PerformWork" => Action::PerformWork {
                facing: f.f32("perform.facing").unwrap_or(0.0),
                anim: int(f, "perform.clip").max(0) as usize,
                duration: time(f, "perform.duration"),
                elapsed: time(f, "perform.elapsed"),
                consume: flag(f, "perform.use_tool"),
            },
            "PlayAnimation" => Action::PlayAnimation {
                anim: int(f, "play.clip").max(0) as usize,
                duration: time(f, "play.duration"),
                elapsed: time(f, "play.elapsed"),
            },
            "SendComponentMessage" => {
                let component = component(int(f, "send.component_type") as u32)?;
                // `send.entity` (`+0x20`) is the entity whose component receives the message,
                // `send.argument` (`+0x28`) the value it is given. The port's message holds the
                // target site for a site's component, nothing for the AI, and for a Citizen
                // message the argument: the home or storage it eats from, warms at or is
                // treated at, if any.
                let entity = match component {
                    Component::CitizenAi => None,
                    Component::Citizen => match f.ptr("send.argument") {
                        None => None,
                        Some(_) => Some(placed("send.argument")?),
                    },
                    _ => Some(placed("send.entity")?),
                };
                Action::SendMessage(Message {
                    component,
                    message: int(f, "send.message"),
                    entity,
                    a: int(f, "send.value_38"),
                    b: int(f, "send.value_3c"),
                })
            }
            "ShowTool" => Action::ShowTool(int(f, "show.tool")),
            "CompleteTask" => Action::CompleteTask {
                work: int(f, "complete.work"),
                param: int(f, "complete.param"),
            },
            "CompleteRoad" => Action::CompleteRoad {
                cell: int(f, "road.cell"),
                ty: int(f, "road.ty") as u8,
                built: int(f, "road.built") as u8,
            },
            // No entity, or the citizen's own: what the citizen carries (`usize::MAX`,
            // `FUN_1400cb880` on its own StorageComponent), for example food eaten from its
            // hands. The 1.0.7 saves hold no entity; older saves (Driscola, 2015) hold the
            // citizen's own.
            "ConsumeRawMaterial" => Action::ConsumeStoredRawMaterial {
                storage: match f.ptr("consume.entity") {
                    None => usize::MAX,
                    Some(e) if Some(e) == owner => usize::MAX,
                    Some(_) => storage(self, "consume.entity")?,
                },
                items: items(self, "consume.items")?,
                reserved: flag(f, "consume.flag_38"),
            },
            "PickupStoredRawMaterial" => Action::PickupStoredRawMaterial {
                storage: storage(self, "pickup_stored.storage")?,
                items: items(self, "pickup_stored.items")?,
                partial: flag(f, "pickup_stored.flag_40"),
            },
            // No storage (`usize::MAX`) leaves everything on the ground; no material drops
            // every material carried.
            "DropOffRawMaterial" => Action::DropOff {
                storage: match f.ptr("drop_off.storage") {
                    None => usize::MAX,
                    Some(_) => storage(self, "drop_off.storage")?,
                },
                material: match saved.ext(f, "drop_off.material") {
                    None => usize::MAX,
                    Some(r) => material_index(archive, &mut self.materials, &normalise(r))?,
                },
                count: int(f, "drop_off.count"),
                redirect: flag(f, "drop_off.redirect"),
            },
            "RemoveNaturalResource" => Action::RemoveResource {
                entity: placed("remove.entity")?,
                flag: flag(f, "remove.flag_30"),
            },
            "AnimateNaturalResource" => Action::AnimateResource {
                entity: placed("animate.entity")?,
                anim: int(f, "animate.clip").max(0) as usize,
                wait: flag(f, "animate.flag_2c"),
            },
            "PickupRawMaterial" => Action::PickupPile {
                entity: placed("pickup.entity")?,
            },
            "CreateRawMaterial" => Action::CreateRawMaterial {
                template: load_template(
                    archive,
                    &normalise(saved.ext(f, "create_raw.template")?),
                    &mut self.templates,
                )?,
                cell: int(f, "create_raw.cell"),
                batch: int(f, "create_raw.batch"),
                workplace: placed("create_raw.workplace"),
                factor: f.f32("create_raw.factor").unwrap_or(1.0),
            },
            "CreateNaturalResource" => Action::CreateNaturalResource {
                template: load_template(
                    archive,
                    &normalise(saved.ext(f, "create_natural.template")?),
                    &mut self.templates,
                )?,
                occupant: placed("create_natural.occupant"),
                cell: int(f, "create_natural.cell"),
            },
            // The lodge is the HuntComponent's entity.
            "HuntAnimal" => Action::HuntAnimal {
                lodge: f
                    .ptr("hunt.lodge")
                    .and_then(|h| saved.object(h))
                    .and_then(|h| h.fields.ptr("base.entity"))
                    .and_then(|e| index.get(&e).copied())?,
                done: flag(f, "hunt.done"),
            },
            "RemoveAnimal" => Action::RemoveAnimal {
                entity: placed("remove.entity")?,
            },
            _ => return None,
        })
    }
}

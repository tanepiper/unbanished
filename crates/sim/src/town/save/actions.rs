//! Saving, stage 3b (the inverse of `load::actions`): each character's stack (`+0x48`, top
//! `+0x60`, "on empty" `+0x58`, `FUN_14010ef80`). An entry whose saved action at the same place
//! has the same class keeps that object (its fields rewritten), so an untouched stack keeps its
//! bits; others get a new object.

use unbanished_formats::crc::type_id;
use unbanished_formats::savegame::{Fields, GameState, ObjId, Value};
use unbanished_game::behaviour::character::{Action, ActionStack, Component};

use super::{component, Ids, Refs};
use crate::start::StartingTown;

/// The original's class for an action.
fn action_type(a: &Action) -> &'static str {
    match a {
        Action::MoveTo(_) => "MoveTo",
        Action::PlayAnimation { .. } => "PlayAnimation",
        Action::PerformWork { .. } => "PerformWork",
        Action::SendMessage(_) => "SendComponentMessage",
        Action::CompleteRoad { .. } => "CompleteRoad",
        Action::ConsumeStoredRawMaterial { .. } => "ConsumeRawMaterial",
        Action::PickupStoredRawMaterial { .. } => "PickupStoredRawMaterial",
        Action::ShowTool(_) => "ShowTool",
        Action::CompleteTask { .. } => "CompleteTask",
        Action::AnimateResource { .. } => "AnimateNaturalResource",
        Action::RemoveResource { .. } => "RemoveNaturalResource",
        Action::PickupPile { .. } => "PickupRawMaterial",
        Action::CreateRawMaterial { .. } => "CreateRawMaterial",
        Action::HuntAnimal { .. } => "HuntAnimal",
        Action::RemoveAnimal { .. } => "RemoveAnimal",
        Action::CreateNaturalResource { .. } => "CreateNaturalResource",
        Action::DropOff { .. } => "DropOffRawMaterial",
    }
}

/// The message target's component class (SendComponentMessage `+0x30`).
fn component_type(c: Component) -> Option<&'static str> {
    Some(match c {
        Component::CitizenAi => "CitizenAIComponent",
        Component::Citizen => "CitizenComponent",
        Component::Happiness => "HappinessComponent",
        Component::Build => "BuildComponent",
        Component::Destroy => "DestroyComponent",
        Component::Fire => "FireComponent",
        Component::CropField => "CropFieldComponent",
        Component::Orchard => "OrchardComponent",
        Component::Pasture => "PastureComponent",
        Component::Quarry => "QuarryComponent",
        Component::Fishing => "FishingComponent",
        _ => return None,
    })
}

/// Whose stack: the entity, its CharacterComponent and (a citizen's) CitizenComponent.
pub(super) struct Owner {
    pub entity: ObjId,
    pub character: ObjId,
    pub citizen: Option<ObjId>,
}

impl StartingTown {
    /// The site entity whose StorageComponent is storage `s`.
    fn storage_entity(&self, ids: &Ids, s: usize) -> Option<ObjId> {
        let e = self
            .construction
            .sites
            .iter()
            .find(|(_, site)| site.storage == s)
            .map(|(&e, _)| e)?;
        ids.entity(e)
    }

    /// Write `stack` into the owner's CharacterComponent.
    pub(super) fn export_stack(
        &self,
        g: &mut GameState,
        refs: &mut Refs,
        ids: &Ids,
        owner: &Owner,
        stack: &ActionStack,
    ) {
        let (entries, on_empty) = stack.saved();
        let saved_stack: Vec<Option<ObjId>> = match g
            .objects
            .get(&owner.character)
            .and_then(|o| o.fields.at("character.stack"))
        {
            Some(Value::List(items)) => items.iter().map(|i| i.ptr("action")).collect(),
            _ => Vec::new(),
        };
        let saved_on_empty = g
            .objects
            .get(&owner.character)
            .and_then(|o| o.fields.ptr("character.on_empty"));
        let mut list = Vec::new();
        for (k, (action, started)) in entries.iter().enumerate() {
            let reuse = saved_stack.get(k).copied().flatten();
            list.push(self.action_object(g, refs, ids, owner, action, Some(*started), reuse));
        }
        // The array keeps its saved length (unused slots null), at least the entries.
        let len = saved_stack.len().max(list.len());
        list.resize(len, None);
        let on_empty =
            on_empty.and_then(|a| self.action_object(g, refs, ids, owner, a, None, saved_on_empty));
        let top = entries.len() as i32 - 1;
        if let Some(o) = g.objects.get_mut(&owner.character) {
            let f = &mut o.fields;
            f.set_i32("character.stack_top", top);
            f.set(
                "character.stack",
                Value::List(
                    list.into_iter()
                        .map(|a| Fields(vec![("action", Value::Ptr(a))]))
                        .collect(),
                ),
            );
            f.set("character.on_empty", Value::Ptr(on_empty));
        }
    }

    /// One action's object: the saved one at its place if it is of the same class, else new.
    #[allow(clippy::too_many_arguments)]
    fn action_object(
        &self,
        g: &mut GameState,
        refs: &mut Refs,
        ids: &Ids,
        owner: &Owner,
        action: &Action,
        started: Option<bool>,
        reuse: Option<ObjId>,
    ) -> Option<ObjId> {
        let t = action_type(action);
        let id = match reuse {
            Some(r) if g.objects.get(&r).is_some_and(|o| o.type_name == t) => r,
            _ => g.new_object(t)?,
        };
        // The values that need the game state or the reference table first.
        let entity = |p: usize| ids.entity(p);
        let material =
            |g: &mut GameState, refs: &mut Refs, m: usize| refs.id(g, &self.materials[m].reference);
        let template =
            |g: &mut GameState, refs: &mut Refs, t: usize| refs.id(g, &self.templates[t].reference);
        let items = |g: &mut GameState, refs: &mut Refs, list: &[(usize, i32)]| -> Value {
            Value::List(
                list.iter()
                    .map(|&(m, n)| {
                        Fields(vec![
                            ("material", Value::Ext(material(g, refs, m))),
                            ("count", Value::I32(n)),
                        ])
                    })
                    .collect(),
            )
        };
        let character_description = g
            .objects
            .get(&owner.character)
            .and_then(|o| o.fields.i32("base.description"))
            .unwrap_or(-1);
        let mut set: Vec<(&str, Value)> = Vec::new();
        match action {
            Action::MoveTo(m) => {
                let (index, path) = m.saved_way();
                set.push(("move.owner", Value::Ptr(Some(owner.character))));
                set.push(("move.animations", Value::Ext(character_description as u32)));
                set.push(("move.path_len", Value::I32(path.len() as i32)));
                set.push(("move.path_index", Value::I32(index)));
                set.push((
                    "move.path",
                    Value::U32s(path.iter().map(|&c| c as u32).collect()),
                ));
                set.push(("move.target", Value::F32s(m.target.to_vec())));
                set.push(("move.flags", Value::U32(m.flags)));
                set.push(("move.walk_clip", Value::I32(m.walk as i32)));
                set.push(("move.stand_clip", Value::I32(m.stand as i32)));
                set.push(("move.cost_mode", Value::I32(m.mode as i32)));
                set.push(("move.following", Value::Bool(m.following)));
                set.push(("move.speed_factor", Value::F32(m.speed)));
            }
            Action::PerformWork {
                facing,
                anim,
                duration,
                elapsed,
                consume,
            } => {
                set.push(("perform.owner", Value::Ptr(owner.citizen)));
                set.push(("perform.facing", Value::F32(*facing)));
                set.push(("perform.clip", Value::I32(*anim as i32)));
                set.push(("perform.duration", Value::I64(*duration)));
                set.push(("perform.elapsed", Value::I64(*elapsed)));
                set.push(("perform.use_tool", Value::Bool(*consume)));
            }
            Action::PlayAnimation {
                anim,
                duration,
                elapsed,
            } => {
                set.push(("play.owner", Value::Ptr(Some(owner.entity))));
                set.push(("play.clip", Value::I32(*anim as i32)));
                set.push(("play.duration", Value::I64(*duration)));
                set.push(("play.elapsed", Value::I64(*elapsed)));
            }
            Action::SendMessage(msg) => {
                let target = match msg.component {
                    Component::Citizen | Component::CitizenAi => Some(owner.entity),
                    _ => msg.entity.and_then(entity),
                };
                if let Some(c) = component_type(msg.component) {
                    set.push(("send.component_type", Value::U32(type_id(c))));
                }
                set.push(("send.entity", Value::Ptr(target)));
                set.push(("send.message", Value::I32(msg.message)));
                set.push(("send.value_38", Value::U32(msg.a as u32)));
                set.push(("send.value_3c", Value::U32(msg.b as u32)));
            }
            Action::ShowTool(t) => {
                set.push(("show.owner", Value::Ptr(owner.citizen)));
                set.push(("show.tool", Value::I32(*t)));
            }
            Action::CompleteTask { work, param } => {
                // `+0x28` is the entity its maker passed on to the work's complete slot
                // (`FUN_1400cb270`, `FUN_1400fdc30`); the port does not keep it, so a saved
                // task keeps its own and a new one has none.
                set.push(("complete.citizen", Value::Ptr(owner.citizen)));
                set.push(("complete.param", Value::I32(*param)));
                set.push(("complete.work", Value::I32(*work)));
            }
            Action::CompleteRoad { cell, ty, built } => {
                set.push(("road.cell", Value::I32(*cell)));
                set.push(("road.built", Value::I8(*built as i8)));
                set.push(("road.ty", Value::I32(*ty as i32)));
            }
            Action::ConsumeStoredRawMaterial {
                storage,
                items: list,
                reserved,
            } => {
                set.push((
                    "consume.entity",
                    Value::Ptr(self.storage_entity(ids, *storage)),
                ));
                set.push(("consume.items", items(g, refs, list)));
                set.push(("consume.flag_38", Value::Bool(*reserved)));
            }
            Action::PickupStoredRawMaterial {
                storage,
                items: list,
                partial,
            } => {
                set.push(("pickup_stored.citizen", Value::Ptr(owner.citizen)));
                set.push((
                    "pickup_stored.storage",
                    Value::Ptr(self.storage_entity(ids, *storage)),
                ));
                set.push(("pickup_stored.items", items(g, refs, list)));
                set.push(("pickup_stored.flag_40", Value::Bool(*partial)));
            }
            Action::DropOff {
                storage,
                material: m,
                count,
                redirect,
            } => {
                let storage = if *storage == usize::MAX {
                    None
                } else {
                    self.storage_entity(ids, *storage)
                };
                let m = if *m == usize::MAX {
                    u32::MAX
                } else {
                    material(g, refs, *m)
                };
                set.push(("drop_off.citizen", Value::Ptr(owner.citizen)));
                set.push(("drop_off.storage", Value::Ptr(storage)));
                set.push(("drop_off.material", Value::Ext(m)));
                set.push(("drop_off.redirect", Value::Bool(*redirect)));
                set.push(("drop_off.count", Value::I32(*count)));
            }
            Action::RemoveResource { entity: e, flag } => {
                set.push(("remove.citizen", Value::Ptr(owner.citizen)));
                set.push(("remove.entity", Value::Ptr(entity(*e))));
                set.push(("remove.flag_30", Value::Bool(*flag)));
            }
            Action::AnimateResource {
                entity: e,
                anim,
                wait,
            } => {
                set.push(("animate.entity", Value::Ptr(entity(*e))));
                set.push(("animate.clip", Value::I32(*anim as i32)));
                set.push(("animate.flag_2c", Value::Bool(*wait)));
            }
            Action::PickupPile { entity: e } => {
                set.push(("pickup.citizen", Value::Ptr(owner.citizen)));
                set.push(("pickup.entity", Value::Ptr(entity(*e))));
            }
            Action::CreateRawMaterial {
                template: t,
                cell,
                batch,
                workplace,
                factor,
            } => {
                set.push(("create_raw.template", Value::Ext(template(g, refs, *t))));
                set.push(("create_raw.citizen", Value::Ptr(owner.citizen)));
                set.push(("create_raw.cell", Value::I32(*cell)));
                set.push(("create_raw.factor", Value::F32(*factor)));
                set.push(("create_raw.batch", Value::I32(*batch)));
                set.push((
                    "create_raw.workplace",
                    Value::Ptr(workplace.and_then(entity)),
                ));
            }
            Action::CreateNaturalResource {
                template: t,
                occupant,
                cell,
            } => {
                set.push(("create_natural.template", Value::Ext(template(g, refs, *t))));
                set.push(("create_natural.cell", Value::I32(*cell)));
                set.push((
                    "create_natural.occupant",
                    Value::Ptr(occupant.and_then(entity)),
                ));
            }
            Action::HuntAnimal { lodge, done } => {
                let hunt = entity(*lodge).and_then(|e| component(g, e, "HuntComponent"));
                set.push(("hunt.lodge", Value::Ptr(hunt)));
                set.push(("hunt.hunter", Value::Ptr(owner.citizen)));
                set.push(("hunt.done", Value::Bool(*done)));
            }
            Action::RemoveAnimal { entity: e } => {
                set.push(("remove.entity", Value::Ptr(entity(*e))));
            }
        }
        let f = &mut g.objects.get_mut(&id)?.fields;
        if let Some(started) = started {
            f.set_bool("action.started", started);
        }
        for (path, value) in set {
            f.set(path, value);
        }
        Some(id)
    }
}

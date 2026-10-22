//! Starting an action: what each kind of action does when it begins.

use super::*;

impl Action {
    pub(in crate::behaviour::character) fn start(
        &mut self,
        ch: &mut Character,
        world: &mut World,
        group: &AnimationGroup,
    ) -> Status {
        match self {
            Action::MoveTo(m) => m.start(ch, world, group),
            Action::PlayAnimation { anim, .. } => {
                ch.play(*anim, group, 1.0);
                Status::Running
            }
            Action::PerformWork {
                facing,
                anim,
                consume,
                ..
            } => {
                // Start (slot 4, `FUN_1400ce8f0`): the clip at speed 0 (`FUN_14010f6c0(anim,
                // 1, 0)`), then `FUN_14010f530(FromYawPitch(facing))`.
                ch.play(*anim, group, 0.0);
                if (-180.0..=180.0).contains(facing) {
                    // FUN_14010f530: the direction (x, y, 0) normalized, the heading back
                    // from the vector.
                    let v = crate::math::from_yaw_pitch(*facing, 0.0);
                    ch.flag_28 = false;
                    ch.direction = crate::math::normalize3([v[0], v[1], 0.0]);
                    ch.target_heading = crate::math::heading(v[0], v[1]);
                }
                // `+0x40`: spend one tool use (never below zero).
                if *consume && *world.tool_uses > 0 {
                    *world.tool_uses = (*world.tool_uses - 1).max(0);
                }
                Status::Running
            }
            Action::SendMessage(m) => {
                world.messages.push(*m);
                Status::Done
            }
            Action::AnimateResource { entity, anim, wait } => {
                world.messages.push(Message {
                    component: Component::NaturalResource,
                    message: 2,
                    entity: Some(*entity),
                    a: *anim as i32,
                    b: 0,
                });
                if let Some(a) = world.animating.get_mut(*entity) {
                    *a = true;
                }
                if *wait {
                    Status::Running
                } else {
                    Status::Done
                }
            }
            Action::RemoveResource { entity, flag } => {
                world.messages.push(Message {
                    component: Component::NaturalResource,
                    message: 0,
                    entity: Some(*entity),
                    a: *flag as i32,
                    b: 0,
                });
                Status::Done
            }
            Action::CompleteRoad { cell, ty, built } => {
                world.messages.push(Message {
                    component: Component::Road,
                    message: *built as i32,
                    entity: None,
                    a: *cell,
                    b: *ty as i32,
                });
                Status::Done
            }
            Action::CreateRawMaterial {
                template,
                cell,
                workplace,
                factor,
                ..
            } => {
                world.messages.push(Message {
                    component: Component::CreateRawMaterial,
                    message: factor.to_bits() as i32,
                    entity: *workplace,
                    a: *template as i32,
                    b: *cell,
                });
                Status::Done
            }
            // `FUN_1400cbb20`: the cell must be free (`FUN_1400a1fd0`: not cell types 0 or 1, no
            // road kind `+0x2a`; a road plan always has one) and without flags. Otherwise the
            // action fails.
            Action::CreateNaturalResource {
                template,
                occupant,
                cell,
            } => {
                let i = *cell as usize;
                let free =
                    world.objects.get(i) == Some(&None) && world.occupants.get(i) == Some(occupant);
                let ty = 1u32 << (world.types[i].0 & 0x1f);
                if !free || ty & 3 != 0 || world.roads[i] != 0xff || world.flags[i] != 0 {
                    return Status::Failed;
                }
                world.messages.push(Message {
                    component: Component::CreateNaturalResource,
                    message: 0,
                    entity: None,
                    a: *template as i32,
                    b: *cell,
                });
                Status::Done
            }
            // `FUN_1400cd720`.
            Action::HuntAnimal { done, .. } => {
                if *done {
                    Status::Done
                } else {
                    Status::Running
                }
            }
            Action::RemoveAnimal { entity } => {
                world.messages.push(Message {
                    component: Component::Animal,
                    message: 0,
                    entity: Some(*entity),
                    a: 0,
                    b: 0,
                });
                Status::Done
            }
            Action::PickupPile { entity } => {
                world.messages.push(Message {
                    component: Component::Pile,
                    message: 0,
                    entity: Some(*entity),
                    a: 0,
                    b: 0,
                });
                Status::Done
            }
            Action::ShowTool(n) => {
                // Above 10 (except −2) detaches (`FUN_14011cbb0`); otherwise attach (`FUN_14011c880`).
                *world.shown_tool = if (*n as u32) > 10 && *n != -2 { -1 } else { *n };
                Status::Running
            }
            Action::CompleteTask { work, param } => {
                if *work >= 0 {
                    world.messages.push(Message {
                        component: Component::Jobs,
                        message: 0,
                        entity: None,
                        a: *work,
                        b: *param,
                    });
                }
                Status::Done
            }
            Action::DropOff {
                storage,
                material,
                count,
                redirect,
            } => {
                // `FUN_1400cc7d0`. No storage: everything carried is left on the ground
                // (`FUN_1400cca40`).
                if *storage == usize::MAX {
                    let items: Vec<(usize, i32)> = world
                        .carried
                        .items
                        .iter()
                        .filter(|i| 0 < i.count)
                        .map(|i| (i.material, i.count))
                        .collect();
                    for (m, n) in items {
                        drop_pile(world, m, n);
                    }
                    return Status::Running;
                }
                // Every material tries the storage the action was made for; a redirect points
                // the action at the storage found.
                let target = *storage;
                let one = |world: &mut World, m: usize, storage: &mut usize| -> bool {
                    drop_off_one(ch, world, target, storage, m, *count, *redirect)
                };
                if *material != usize::MAX {
                    let m = *material;
                    return if one(world, m, storage) {
                        Status::Failed
                    } else {
                        Status::Running
                    };
                }
                let items: Vec<usize> = world
                    .carried
                    .items
                    .iter()
                    .filter(|i| 0 < i.count)
                    .map(|i| i.material)
                    .collect();
                let mut dropped = false;
                for m in items {
                    dropped |= one(world, m, storage);
                }
                if dropped {
                    Status::Failed
                } else {
                    Status::Running
                }
            }
            Action::ConsumeStoredRawMaterial {
                storage,
                items,
                reserved,
            } => {
                // Storage `usize::MAX`: what the citizen carries (`FUN_1400cb880` on its own
                // StorageComponent).
                let target = if *storage == usize::MAX {
                    &mut *world.carried
                } else {
                    &mut world.storages[*storage]
                };
                if consume_stored(target, items, *reserved) {
                    // `FUN_1400cb420`: each item's amount counts as used (`+0xc`).
                    world.used.extend(items.iter().copied());
                    Status::Done
                } else {
                    Status::Failed
                }
            }
            Action::PickupStoredRawMaterial {
                storage,
                items,
                partial,
            } => {
                if pickup_stored(
                    &mut world.storages[*storage],
                    world.carried,
                    world.materials,
                    items,
                    *partial,
                ) {
                    Status::Done
                } else {
                    Status::Failed
                }
            }
        }
    }
}

//! The inventory page and the descriptions of its selected goods.

use super::*;

/// `SelectDescription` (serializer `FUN_140055f90`): `+0x20` sprite sheet, `+0x28` sprite,
/// `+0x30` string table, `+0x38` name.
pub(super) fn select_description(
    data: &[u8],
) -> Option<(Option<String>, String, Option<String>, String)> {
    use unbanished_formats::crs;
    crs::parse(data, |r: &mut crs::Reader, _| {
        r.block(|r, _| {
            let v = (
                r.external_ref()?,
                r.string()?,
                r.external_ref()?,
                r.string()?,
            );
            r.skip_rest();
            Ok(v)
        })
    })
    .ok()
    .map(|(_, v)| v)
}

impl TownHallUi {
    /// `FUN_140082a60` / `FUN_140081cc0`: every raw material held in storage (0), trading
    /// posts (1) or homes (2), by name or quantity.
    pub(super) fn inventory(&mut self, ui: &mut Ui, archive: &Archive, town: &StartingTown) {
        let kind = combo_index(ui, self.inventory_type);
        let by_count = combo_index(ui, self.sort) == 1;
        let mut held = vec![0i32; town.materials.len()];
        let mut add = |storage: usize| {
            if let Some(s) = town.storages.get(storage) {
                for item in &s.items {
                    if let Some(h) = held.get_mut(item.material) {
                        *h += item.count;
                    }
                }
            }
        };
        match kind {
            0 => {
                for s in &town.storage_sites {
                    let trading = town
                        .construction
                        .sites
                        .get(&s.entity)
                        .is_some_and(|x| x.trade.is_some());
                    if !trading {
                        add(s.storage);
                    }
                }
            }
            1 => {
                for &p in &town.construction.trading_posts {
                    if let Some(site) = town.construction.sites.get(&p) {
                        add(site.storage);
                    }
                }
            }
            _ => {
                for home in town.construction.homes.values() {
                    add(home.storage);
                }
            }
        }
        let mut rows: Vec<(String, (Option<String>, String), i32)> = Vec::new();
        for (m, &count) in held.iter().enumerate() {
            let file = town.materials[m]
                .reference
                .split(':')
                .next()
                .unwrap_or("")
                .replace('\\', "/");
            let Some(v) = archive
                .read_ref(&format!("{file}:value"))
                .ok()
                .and_then(|d| unbanished_formats::game::value_description(&d).ok())
            else {
                continue;
            };
            let name = ui.res.string(v.name.0.as_deref(), &v.name.1);
            rows.push((name, v.icon.clone(), count));
        }
        // The CRT qsort: by quantity, most first, then name (`FUN_140015f10` with
        // `FUN_140081af0`), or by name (`FUN_140015c10` with `FUN_140081be0`).
        if by_count {
            unbanished_game::behaviour::ai::crt_qsort_less(&mut rows, |a, b| {
                b.2 < a.2 || (a.2 == b.2 && crate::ui::building_ui::str_less(&a.0, &b.0))
            });
        } else {
            unbanished_game::behaviour::ai::crt_qsort_less(&mut rows, |a, b| {
                crate::ui::building_ui::str_less(&a.0, &b.0)
            });
        }
        for (i, (label, number)) in self.inventory.iter().enumerate() {
            match rows.get(i) {
                Some((name, icon, count)) => {
                    if let Some(l) = *label {
                        let sprite = ui.res.sprite(icon.0.as_deref(), &icon.1);
                        if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                            *icon = sprite;
                        }
                        ui.set_text(l, name);
                    }
                    set_number(ui, *number, *count);
                }
                None => {
                    if let Some(l) = *label {
                        if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                            *icon = None;
                        }
                        ui.set_text(l, "");
                    }
                    if let Some(n) = *number {
                        ui.set_text(n, "");
                    }
                }
            }
        }
    }
}

//! Stage 3d of loading a save (docs/SAVE-LOADING.md): the buildings' own component state,
//! field by field from each serializer, onto the port's site fields (their offsets are in the
//! port's doc comments): production (ConsumeProduce and ResourceProduce), forester, hunting,
//! fishing, crop fields and orchards (and the fruit on their trees), auto pickup, MultiObject
//! parts, happiness, cemeteries, diseases, demolition, fire, the town hall, trading posts,
//! trackers, floors, terraforming and tombstones.

use std::collections::BTreeMap;

use banished_formats::pkg::Archive;
use banished_formats::savegame::{Fields, ObjId, Value};

use super::super::{load_template, material_index, normalise, StartingTown};
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

fn ids(f: &Fields, path: &str) -> Vec<u32> {
    match f.at(path) {
        Some(Value::U32s(v)) => v.clone(),
        _ => Vec::new(),
    }
}

impl StartingTown {
    pub(in crate::start) fn restore_producers(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
    ) {
        let entity_of = |id: Option<ObjId>| entity_or_component(saved, index, id);
        let path_of = |id: u32| saved.reference(id).map(normalise);
        if saved
            .entities()
            .any(|(_, e)| saved.component(e, "TradeComponent").is_some())
        {
            self.load_trade_goods(archive);
        }
        let mut skipped: BTreeMap<&str, usize> = BTreeMap::new();
        for (id, e) in saved.entities() {
            let Some(&entity) = index.get(&id) else {
                continue;
            };
            let component = |name: &str| saved.component(e, name).map(|c| &c.fields);

            // Components on the entity whatever it is.
            if let Some(f) = component("FruitComponent") {
                self.construction.fruits.insert(
                    entity,
                    banished_game::orchard::Fruit {
                        ripeness: f.f32("fruit.value_30").unwrap_or(0.0),
                        picked: flag(f, "fruit.flag_34"),
                    },
                );
            }
            if let Some(f) = component("TrackerComponent") {
                let amounts = |name: &str, town: &mut Self| -> Vec<(usize, i32)> {
                    list(f, &format!("tracker.{name}"))
                        .iter()
                        .filter_map(|i| {
                            let m = material_index(
                                archive,
                                &mut town.materials,
                                &normalise(saved.ext(i, "material")?),
                            )?;
                            Some((m, int(i, "amount")))
                        })
                        .collect()
                };
                let this_year = amounts("this_year", self);
                let last_year = amounts("last_year", self);
                self.construction.trackers.insert(
                    entity,
                    banished_game::tracker::Tracker {
                        this_year,
                        last_year,
                        // −1 (never) stays distinct from every year.
                        year: int(f, "tracker.year") as u32,
                    },
                );
            }
            if let Some(f) = component("FloorComponent") {
                if flag(f, "floor.active") {
                    self.add_floor(entity);
                }
            }
            if let Some(f) = component("TerraformComponent") {
                self.construction.terraforms.insert(
                    entity,
                    banished_game::terraform::TerraformState {
                        raised: list(f, "terraform.points")
                            .iter()
                            .filter_map(|p| match p.get("point") {
                                Some(Value::F32s(v)) if v.len() == 3 => Some([v[0], v[1], v[2]]),
                                _ => None,
                            })
                            .collect(),
                        // Inferred: the cells it hardened are not saved; none are kept.
                        cells: Vec::new(),
                    },
                );
            }
            if let Some(f) = component("TombstoneComponent") {
                self.construction
                    .tombstones
                    .insert(entity, time(f, "tombstone.left"));
            }
            if component("NomadComponent").is_some() {
                *skipped.entry("NomadComponent").or_default() += 1;
            }
            if component("TornadoComponent").is_some() {
                *skipped.entry("TornadoComponent").or_default() += 1;
            }

            // The site's components.
            if !self.construction.sites.contains_key(&entity) {
                continue;
            }
            let disabled = component("ResourceProduceComponent")
                .or(component("ConsumeProduceComponent"))
                .or(component("FishingComponent"))
                .or(component("ForesterComponent"))
                .or(component("HerbalistComponent"))
                .or(component("HuntComponent"))
                .or(component("MarketComponent"))
                .map(|f| flag(f, "produce.disabled"));
            let crop_ref = component("CropFieldComponent").and_then(|f| saved.ext(f, "field.crop"));
            let crop =
                crop_ref.and_then(|r| load_template(archive, &normalise(r), &mut self.templates));
            let tree_ref = component("OrchardComponent").and_then(|f| saved.ext(f, "orchard.tree"));
            let tree =
                tree_ref.and_then(|r| load_template(archive, &normalise(r), &mut self.templates));
            let rebuild_ref =
                component("DestroyComponent").and_then(|f| saved.ext(f, "destroy.rebuild"));
            let rebuild = rebuild_ref
                .and_then(|r| load_template(archive, &normalise(r), &mut self.templates));
            let blight_ref =
                component("DiseaseComponent").and_then(|f| saved.ext(f, "disease.disease"));
            let blight_last =
                blight_ref.and_then(|r| load_template(archive, &normalise(r), &mut self.templates));
            let trade =
                component("TradeComponent").map(|f| self.saved_trade(archive, saved, index, f));
            let site = self.construction.sites.get_mut(&entity).unwrap();
            if let (Some(p), Some(disabled)) = (site.produce.as_mut(), disabled) {
                p.disabled = disabled;
            }
            if let (Some(p), Some(f)) =
                (site.produce.as_mut(), component("ConsumeProduceComponent"))
            {
                p.selector = int(f, "consume_produce.selector");
                if let Some(Value::F32s(w)) = f.at("consume_produce.weights") {
                    p.weights = w.clone();
                }
                p.produced = int(f, "consume_produce.produced");
            }
            if let (Some(fo), Some(f)) = (site.forester.as_mut(), component("ForesterComponent")) {
                fo.plant = flag(f, "forester.plant");
                fo.cut = flag(f, "forester.cut");
            }
            if let Some(f) = component("HuntComponent") {
                site.hunt_time = Some(time(f, "hunt.time_50"));
            }
            if let (Some(fi), Some(f)) = (site.fishing.as_mut(), component("FishingComponent")) {
                fi.registered = flag(f, "fishing.flag_50");
                fi.water = f.f32("fishing.value_54").unwrap_or(0.0);
            }
            if let (Some(cf), Some(f)) = (site.crop_field.as_mut(), component("CropFieldComponent"))
            {
                cf.enabled = flag(f, "field.flag_48");
                cf.piles = flag(f, "field.flag_49");
                cf.crop = crop;
                cf.cells = list(f, "field.cells")
                    .iter()
                    .map(|c| banished_game::farm::CropCell {
                        cell: int(c, "x"),
                        state: int(c, "y"),
                        entity: entity_of(c.ptr("entity")),
                        pile: flag(c, "flag"),
                    })
                    .collect();
            }
            if let (Some(o), Some(f)) = (site.orchard.as_mut(), component("OrchardComponent")) {
                o.enabled = flag(f, "orchard.flag_48");
                o.year = int(f, "orchard.value_4c") as u32;
                o.tree = tree;
                o.trees = list(f, "orchard.trees")
                    .iter()
                    .map(|t| banished_game::orchard::TreeSlot {
                        cell: int(t, "x"),
                        state: int(t, "y"),
                        entity: entity_of(t.ptr("entity")),
                        picked: flag(t, "flag"),
                    })
                    .collect();
            }
            if let (Some(a), Some(f)) =
                (site.auto_pickup.as_mut(), component("AutoPickupComponent"))
            {
                a.enabled = flag(f, "pickup.on");
                // `+0x30`: the Pickup works of the piles the workers made.
                a.works = ids(f, "pickup.cells")
                    .into_iter()
                    .map(|w| w as i32)
                    .collect();
            }
            if let Some(f) = component("MultiObjectComponent") {
                site.parts = list(f, "multi.parts")
                    .iter()
                    .filter_map(|p| entity_of(p.ptr("part")))
                    .collect();
                site.multi_stages = Some(int(f, "multi.stages"));
            }
            if let Some(f) = component("HappinessComponent") {
                site.happiness_registered = flag(f, "happiness.registered");
            }
            if let Some(f) = component("CemeteryComponent") {
                site.graves = ids(f, "cemetery.graves")
                    .into_iter()
                    .map(|g| g as i32)
                    .collect();
            }
            if let (Some(b), Some(f)) = (site.disease.as_mut(), component("DiseaseComponent")) {
                b.check = time(f, "disease.time_28");
                b.left = time(f, "disease.time_30");
                b.kill = time(f, "disease.time_38");
                b.spread = time(f, "disease.time_40");
                b.pasture = int(f, "disease.value_48") != 0;
                b.last = blight_last;
            }
            if let (Some(d), Some(f)) = (site.destroy.as_mut(), component("DestroyComponent")) {
                d.count = int(f, "destroy.count");
                d.target = int(f, "destroy.target");
                d.refund = flag(f, "destroy.refund");
                d.rebuild = rebuild;
                d.damaged = flag(f, "destroy.damaged");
                d.time = time(f, "destroy.time");
                // Inferred: the damaged material set shows while damaged.
                d.damaged_look = d.damaged;
            }
            if let (Some(fire), Some(f)) = (site.fire.as_mut(), component("FireComponent")) {
                fire.burn = time(f, "fire.burn");
                fire.spread = time(f, "fire.spread");
                fire.buckets = int(f, "fire.buckets");
                fire.buckets_seen = int(f, "fire.value_2c");
            }
            if let (Some(im), Some(f)) =
                (site.immigration.as_mut(), component("ImmigrationComponent"))
            {
                im.arrived = int(f, "immigration.value_50");
                im.timer = time(f, "immigration.time_48");
                // The band's nomads are not restored yet.
                im.nomads = Vec::new();
            }
            if trade.is_some() {
                site.trade = trade;
            }
        }
        // NaturalResourceComponent `+0x2d` on a crop field's crops: harvesting leaves a pile.
        let crops: Vec<usize> = self
            .construction
            .sites
            .values()
            .filter_map(|s| s.crop_field.as_ref())
            .flat_map(|f| f.cells.iter().filter_map(|c| c.entity))
            .collect();
        for (id, e) in saved.entities() {
            let Some(&entity) = index.get(&id) else {
                continue;
            };
            if !crops.contains(&entity) {
                continue;
            }
            if let Some(n) = saved.component(e, "NaturalResourceComponent") {
                self.construction
                    .keep_piles
                    .insert(entity, flag(&n.fields, "natural.flag_2d"));
            }
        }
        if !skipped.is_empty() {
            log::warn!("components not restored yet: {skipped:?}");
        }
        let _ = path_of;
    }

    /// A TradeComponent (`FUN_14005c6b0`): the merchant's goods (`+0x50`), the merchant docked
    /// (`+0x2c`), the goods to keep (`+0x60`), the orders by merchant (`+0x30`), the automatic
    /// trades (`+0x40`, written `+0`, `+8`, `+4`), the mode (`+0x28`), the clock (`+0x78`),
    /// held (`+0x71`), the boat (`+0x80`) and TradeWork on (`+0x70`).
    fn saved_trade(
        &mut self,
        archive: &Archive,
        saved: &SavedGame,
        index: &BTreeMap<ObjId, usize>,
        f: &Fields,
    ) -> banished_game::trade::Trade {
        let good = |town: &Self, r: &str| {
            let key = normalise(r);
            town.construction
                .trade_goods
                .iter()
                .position(|g| town.templates[g.template].reference == key)
        };
        let offers = list(f, "trade.stock")
            .iter()
            .filter_map(|o| {
                Some(banished_game::trade::Offer {
                    good: good(self, saved.ext(o, "material")?)?,
                    value: int(o, "value_4"),
                    amount: int(o, "value_8"),
                })
            })
            .collect();
        let keep = list(f, "trade.requests")
            .iter()
            .filter_map(|k| {
                let m = material_index(
                    archive,
                    &mut self.materials,
                    &normalise(saved.ext(k, "material")?),
                )?;
                Some((m, int(k, "count")))
            })
            .collect();
        let orders = list(f, "trade.merchants")
            .iter()
            .map(|o| banished_game::trade::Order {
                name: match o.get("name") {
                    Some(Value::Str(s)) => s.clone(),
                    _ => String::new(),
                },
                goods: ids(o, "materials")
                    .into_iter()
                    .filter_map(|id| good(self, saved.reference(id)?))
                    .collect(),
                state: int(o, "value_18"),
            })
            .collect();
        let automatic = list(f, "trade.orders")
            .iter()
            .map(|a| {
                (
                    int(a, "value_0") as u32,
                    int(a, "value_4"),
                    int(a, "value_8"),
                )
            })
            .collect();
        banished_game::trade::Trade {
            mode: int(f, "trade.value_28"),
            merchant: int(f, "trade.value_2c"),
            orders,
            automatic,
            offers,
            keep,
            working: flag(f, "trade.flag_70"),
            held: flag(f, "trade.flag_71"),
            timer: time(f, "trade.time_78"),
            boat: entity_or_component(saved, index, f.ptr("trade.merchant")),
        }
    }
}

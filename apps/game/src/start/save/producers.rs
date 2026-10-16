//! Saving, stage 3d (the inverse of `load::producers`): the buildings' own component state,
//! the fruit on orchard trees, trackers, floors, terraform points, tombstones and the crops'
//! pile flag.

use banished_formats::savegame::{Fields, GameState, Value};

use super::construction::{in_saved_order, like_saved, saved_order};
use super::{component, fields_mut, Ids, Refs};
use crate::start::StartingTown;

impl StartingTown {
    pub(super) fn export_producers(&self, g: &mut GameState, refs: &mut Refs, ids: &Ids) {
        let entity = |p: usize| ids.entity(p);
        let c = &self.construction;
        for (&e, fruit) in &c.fruits {
            if let Some(f) = entity(e).and_then(|id| fields_mut(g, id, "FruitComponent")) {
                f.set_f32("fruit.value_30", fruit.ripeness);
                f.set_bool("fruit.flag_34", fruit.picked);
            }
        }
        for (&e, t) in &c.trackers {
            let amounts = |g: &mut GameState, refs: &mut Refs, list: &[(usize, i32)]| {
                Value::List(
                    list.iter()
                        .map(|&(m, n)| {
                            Fields(vec![
                                (
                                    "material",
                                    Value::Ext(refs.id(g, &self.materials[m].reference)),
                                ),
                                ("amount", Value::I32(n)),
                            ])
                        })
                        .collect(),
                )
            };
            let this_year = amounts(g, refs, &t.this_year);
            let last_year = amounts(g, refs, &t.last_year);
            if let Some(f) = entity(e).and_then(|id| fields_mut(g, id, "TrackerComponent")) {
                f.set("tracker.this_year", this_year);
                f.set("tracker.last_year", last_year);
                f.set_i32("tracker.year", t.year as i32);
            }
        }
        for (p, _) in self.placed.iter().enumerate() {
            if let Some(f) = entity(p).and_then(|id| fields_mut(g, id, "FloorComponent")) {
                f.set_bool("floor.active", self.cells.floors.meshes.contains_key(&p));
            }
        }
        for (&e, t) in &c.terraforms {
            if let Some(f) = entity(e).and_then(|id| fields_mut(g, id, "TerraformComponent")) {
                f.set(
                    "terraform.points",
                    Value::List(
                        t.raised
                            .iter()
                            .map(|p| Fields(vec![("point", Value::F32s(p.to_vec()))]))
                            .collect(),
                    ),
                );
            }
        }
        for (&e, &left) in &c.tombstones {
            if let Some(f) = entity(e).and_then(|id| fields_mut(g, id, "TombstoneComponent")) {
                f.set_time("tombstone.left", left);
            }
        }
        for (&e, &keep) in &c.keep_piles {
            if let Some(f) = entity(e).and_then(|id| fields_mut(g, id, "NaturalResourceComponent"))
            {
                f.set_bool("natural.flag_2d", keep);
            }
        }
        for (&e, site) in &c.sites {
            let Some(id) = entity(e) else { continue };
            let template = |g: &mut GameState, refs: &mut Refs, t: Option<usize>| {
                t.map_or(u32::MAX, |t| refs.id(g, &self.templates[t].reference)) as i32
            };
            let crop = template(g, refs, site.crop_field.as_ref().and_then(|f| f.crop));
            let tree = template(g, refs, site.orchard.as_ref().and_then(|o| o.tree));
            let rebuild = template(g, refs, site.destroy.as_ref().and_then(|d| d.rebuild));
            let blight_last = template(g, refs, site.disease.as_ref().and_then(|b| b.last));
            if let Some(p) = &site.produce {
                for t in [
                    "ConsumeProduceComponent",
                    "FishingComponent",
                    "ForesterComponent",
                    "HerbalistComponent",
                    "HuntComponent",
                    "MarketComponent",
                ] {
                    if let Some(f) = fields_mut(g, id, t) {
                        f.set_bool("produce.disabled", p.disabled);
                    }
                }
                if let Some(f) = fields_mut(g, id, "ConsumeProduceComponent") {
                    f.set_i32("consume_produce.selector", p.selector);
                    f.set_i32("consume_produce.weight_count", p.weights.len() as i32);
                    f.set("consume_produce.weights", Value::F32s(p.weights.clone()));
                    f.set_i32("consume_produce.produced", p.produced);
                }
            }
            if let (Some(fo), Some(f)) = (&site.forester, fields_mut(g, id, "ForesterComponent")) {
                f.set_bool("forester.plant", fo.plant);
                f.set_bool("forester.cut", fo.cut);
            }
            if let (Some(t), Some(f)) = (site.hunt_time, fields_mut(g, id, "HuntComponent")) {
                f.set_time("hunt.time_50", t);
            }
            if let (Some(fi), Some(f)) = (&site.fishing, fields_mut(g, id, "FishingComponent")) {
                f.set_bool("fishing.flag_50", fi.registered);
                f.set_f32("fishing.value_54", fi.water);
            }
            // A cell's crop: its entity and that entity's NaturalResourceComponent.
            let crop_cells = site.crop_field.as_ref().map(|cf| {
                Value::List(
                    cf.cells
                        .iter()
                        .map(|cell| {
                            let e = cell.entity.and_then(entity);
                            let resource =
                                e.and_then(|e| component(g, e, "NaturalResourceComponent"));
                            Fields(vec![
                                ("x", Value::I32(cell.cell)),
                                ("y", Value::I32(cell.state)),
                                ("entity", Value::Ptr(e)),
                                ("resource", Value::Ptr(resource)),
                                ("flag", Value::Bool(cell.pile)),
                            ])
                        })
                        .collect(),
                )
            });
            // A tree: its entity, NaturalResourceComponent and FruitComponent.
            let tree_slots = site.orchard.as_ref().map(|o| {
                Value::List(
                    o.trees
                        .iter()
                        .map(|t| {
                            let e = t.entity.and_then(entity);
                            let resource =
                                e.and_then(|e| component(g, e, "NaturalResourceComponent"));
                            let fruit = e.and_then(|e| component(g, e, "FruitComponent"));
                            Fields(vec![
                                ("x", Value::I32(t.cell)),
                                ("y", Value::I32(t.state)),
                                ("entity", Value::Ptr(e)),
                                ("resource", Value::Ptr(resource)),
                                ("fruit", Value::Ptr(fruit)),
                                ("flag", Value::Bool(t.picked)),
                            ])
                        })
                        .collect(),
                )
            });
            if let (Some(cf), Some(f)) = (&site.crop_field, fields_mut(g, id, "CropFieldComponent"))
            {
                f.set_bool("field.flag_48", cf.enabled);
                f.set_bool("field.flag_49", cf.piles);
                f.set_i32("field.crop", crop);
                if let Some(cells) = crop_cells {
                    f.set("field.cells", cells);
                }
            }
            if let (Some(o), Some(f)) = (&site.orchard, fields_mut(g, id, "OrchardComponent")) {
                f.set_bool("orchard.flag_48", o.enabled);
                f.set_i32("orchard.value_4c", o.year as i32);
                f.set_i32("orchard.tree", tree);
                if let Some(trees) = tree_slots {
                    f.set("orchard.trees", trees);
                }
            }
            if let (Some(a), Some(f)) =
                (&site.auto_pickup, fields_mut(g, id, "AutoPickupComponent"))
            {
                f.set_bool("pickup.on", a.enabled);
                f.set_i32("pickup.cell_count", a.works.len() as i32);
                f.set(
                    "pickup.cells",
                    Value::U32s(a.works.iter().map(|&w| w as u32).collect()),
                );
            }
            if let Some(stages) = site.multi_stages {
                let parts: Vec<Option<i32>> = site.parts.iter().map(|&p| entity(p)).collect();
                if let Some(f) = fields_mut(g, id, "MultiObjectComponent") {
                    let parts = like_saved(
                        f.at("multi.parts"),
                        Value::List(
                            parts
                                .into_iter()
                                .map(|p| Fields(vec![("part", Value::Ptr(p))]))
                                .collect(),
                        ),
                    );
                    f.set("multi.parts", parts);
                    f.set_i32("multi.stages", stages);
                }
            }
            if let Some(f) = fields_mut(g, id, "HappinessComponent") {
                f.set_bool("happiness.registered", site.happiness_registered);
            }
            if let Some(f) = fields_mut(g, id, "CemeteryComponent") {
                f.set_i32("cemetery.count", site.graves.len() as i32);
                f.set(
                    "cemetery.graves",
                    Value::U32s(site.graves.iter().map(|&g| g as u32).collect()),
                );
            }
            if let (Some(b), Some(f)) = (&site.disease, fields_mut(g, id, "DiseaseComponent")) {
                f.set_time("disease.time_28", b.check);
                f.set_time("disease.time_30", b.left);
                f.set_time("disease.time_38", b.kill);
                f.set_time("disease.time_40", b.spread);
                f.set_i32("disease.value_48", b.pasture as i32);
                f.set_i32("disease.disease", blight_last);
            }
            if let (Some(d), Some(f)) = (&site.destroy, fields_mut(g, id, "DestroyComponent")) {
                f.set_i32("destroy.count", d.count);
                f.set_i32("destroy.target", d.target);
                f.set_bool("destroy.refund", d.refund);
                f.set_i32("destroy.rebuild", rebuild);
                f.set_bool("destroy.damaged", d.damaged);
                f.set_time("destroy.time", d.time);
            }
            if let (Some(fire), Some(f)) = (&site.fire, fields_mut(g, id, "FireComponent")) {
                f.set_time("fire.burn", fire.burn);
                f.set_time("fire.spread", fire.spread);
                f.set_i32("fire.buckets", fire.buckets);
                f.set_i32("fire.value_2c", fire.buckets_seen);
            }
            if let (Some(im), Some(f)) =
                (&site.immigration, fields_mut(g, id, "ImmigrationComponent"))
            {
                f.set_i32("immigration.value_50", im.arrived);
                f.set_time("immigration.time_48", im.timer);
            }
            if let Some(t) = &site.trade {
                self.export_trade(g, refs, ids, id, t);
            }
        }
    }

    /// A TradeComponent (`FUN_14005c6b0`, the inverse of `load::producers::saved_trade`).
    fn export_trade(
        &self,
        g: &mut GameState,
        refs: &mut Refs,
        ids: &Ids,
        id: i32,
        t: &banished_game::trade::Trade,
    ) {
        let good = |g: &mut GameState, refs: &mut Refs, k: usize| {
            let r = &self.templates[self.construction.trade_goods[k].template].reference;
            refs.id(g, r)
        };
        let offers: Vec<Fields> = t
            .offers
            .iter()
            .map(|o| {
                Fields(vec![
                    ("material", Value::Ext(good(g, refs, o.good))),
                    ("value_4", Value::I32(o.value)),
                    ("value_8", Value::I32(o.amount)),
                ])
            })
            .collect();
        // A map keyed by the material's address: kept in the saved order.
        let keep: Vec<(u32, Fields)> = t
            .keep
            .iter()
            .map(|(&m, &n)| {
                let r = refs.id(g, &self.materials[m].reference);
                (
                    r,
                    Fields(vec![("material", Value::Ext(r)), ("count", Value::I32(n))]),
                )
            })
            .collect();
        let order = component(g, id, "TradeComponent")
            .and_then(|c| g.objects.get(&c))
            .map(|o| saved_order(o.fields.at("trade.requests"), "material"))
            .unwrap_or_default();
        let keep = in_saved_order(&order, keep);
        let orders: Vec<Fields> = t
            .orders
            .iter()
            .map(|o| {
                let goods: Vec<u32> = o.goods.iter().map(|&k| good(g, refs, k)).collect();
                Fields(vec![
                    ("name", Value::Str(o.name.clone())),
                    ("count", Value::I32(goods.len() as i32)),
                    ("materials", Value::U32s(goods)),
                    ("value_18", Value::I32(o.state)),
                ])
            })
            .collect();
        let automatic: Vec<Fields> = t
            .automatic
            .iter()
            .map(|&(flags, index, amount)| {
                Fields(vec![
                    ("value_0", Value::U32(flags)),
                    ("value_8", Value::I32(amount)),
                    ("value_4", Value::I32(index)),
                ])
            })
            .collect();
        let boat = t.boat.and_then(|b| ids.entity(b));
        let Some(f) = fields_mut(g, id, "TradeComponent") else {
            return;
        };
        f.set("trade.stock", Value::List(offers));
        f.set_i32("trade.value_2c", t.merchant);
        f.set("trade.requests", Value::List(keep));
        f.set("trade.merchants", Value::List(orders));
        f.set("trade.orders", Value::List(automatic));
        f.set_i32("trade.value_28", t.mode);
        f.set_time("trade.time_78", t.timer);
        f.set_bool("trade.flag_71", t.held);
        f.set("trade.merchant", Value::Ptr(boat));
        f.set_bool("trade.flag_70", t.working);
    }
}

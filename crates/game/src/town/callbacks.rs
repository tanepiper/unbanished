//! The building state callbacks ([`Callbacks`]) that construction and destruction call.

use super::*;

mod building;

impl Callbacks for Sim<'_, '_> {
    fn machine(&mut self, site: usize) -> &mut Machine {
        &mut self.sites.get_mut(&site).unwrap().machine
    }

    fn call(&mut self, jobs: &mut Jobs, e: usize, f: Callback) -> bool {
        match f {
            Callback::None | Callback::Missing => false,
            Callback::ClearEnter0 => {
                // `FUN_14011ce30`: with the description's instant flag (`+0x2d`; or the debug
                // flag `Game +0xd0` bit 0) the footprint is emptied at once (`FUN_14011d230`).
                let instant = self
                    .sites
                    .get(&e)
                    .and_then(|s| self.site_templates[s.template].clear.as_ref())
                    .is_some_and(|d| d.instant);
                if instant {
                    self.clear_instantly(jobs, e);
                }
                if self.is_clear(e) {
                    self.machine(e).request(1);
                } else {
                    self.register_clear(jobs, e);
                }
                true
            }
            Callback::ClearUpdate0 => {
                if self.is_clear(e) {
                    self.machine(e).request(1);
                }
                false
            }
            Callback::ClearExit0 => {
                // `FUN_14011cd60`: remove the ClearArea work.
                if self.sites[&e].work_id >= 0 && jobs.works.contains_key(&self.sites[&e].work_id) {
                    self.remove_work(jobs, e);
                }
                true
            }
            // `FUN_14011cc40`: the ClearArea work goes.
            Callback::ClearExit15 => {
                let id = self.sites[&e].work_id;
                if id >= 0 && jobs.works.contains_key(&id) {
                    self.remove_work(jobs, e);
                }
                true
            }
            Callback::OrchardEnter2 => {
                self.orchard_enter2(e);
                false
            }
            Callback::OrchardUpdate2 => {
                self.orchard_update2(e);
                true
            }
            Callback::OrchardEnter9 => {
                self.orchard_enter9(jobs, e);
                true
            }
            Callback::OrchardUpdate9 => {
                self.orchard_update9(jobs, e);
                true
            }
            Callback::OrchardEnter10 => {
                self.orchard_enter10(jobs, e);
                true
            }
            Callback::OrchardUpdate10 => {
                self.orchard_update10(jobs, e);
                true
            }
            Callback::OrchardExitWork => {
                self.orchard_exit_work(jobs, e);
                true
            }
            Callback::OrchardEnter12 => {
                self.orchard_enter12(jobs, e);
                true
            }
            Callback::OrchardUpdate12 => {
                self.orchard_update12(e);
                true
            }
            Callback::OrchardEnter11 => {
                self.orchard_enter11(jobs, e);
                true
            }
            Callback::OrchardUpdate11 => {
                self.orchard_update11(jobs, e);
                true
            }
            Callback::OrchardEnter14 => {
                self.orchard_enter14(jobs, e);
                true
            }
            Callback::OrchardExit14 => {
                self.orchard_exit14(jobs, e);
                true
            }
            Callback::OrchardUpdate14 => {
                self.orchard_update14(e);
                true
            }
            Callback::CropEnter2 => {
                self.crop_enter2(e);
                false
            }
            Callback::CropUpdate2 => {
                self.crop_update2(e);
                true
            }
            Callback::CropEnter9 => {
                self.crop_enter9(jobs, e);
                true
            }
            Callback::CropUpdate9 => {
                self.crop_update9(jobs, e);
                true
            }
            Callback::CropEnter10 => {
                self.crop_enter10(jobs, e);
                true
            }
            Callback::CropExit10 => {
                self.crop_exit10(jobs, e);
                true
            }
            Callback::CropUpdate10 => {
                self.crop_update10(e);
                true
            }
            Callback::CropEnter11 => {
                self.crop_enter11(jobs, e);
                true
            }
            Callback::CropUpdate11 => {
                self.crop_update11(jobs, e);
                true
            }
            Callback::ClearEnter1 => {
                // `FUN_14011ccf0`: the ClearArea stamps the terrain, then on to 2.
                self.clear_area_stamp(e);
                self.machine(e).request(2);
                true
            }
            Callback::ClearEnter2 => {
                // Entity event 1: the MapComponent stamps the footprint; the MultiObject passes
                // it on to the parts (`FUN_14012a960`); the decal turns opaque.
                self.stamp(e);
                self.decal_opaque(e);
                for part in self.sites[&e].parts.clone() {
                    self.stamp(part);
                    self.decal_opaque(part);
                }
                true
            }
            Callback::AbsorbUpdate0 => {
                self.absorb_piles(jobs, e);
                true
            }
            // The market's storage joins the town's markets (with its radius).
            Callback::MarketEnter2 => {
                self.storage_changes.push((e, true));
                false
            }
            Callback::MarketEnter3 => {
                self.storage_changes.push((e, false));
                false
            }
            Callback::CemeteryEnter2 => {
                self.cemetery_enter2(e);
                false
            }
            Callback::CemeteryEnter3 => {
                self.cemeteries.retain(|&c| c != e);
                false
            }
            Callback::StorageEnter2 => {
                self.storage_changes.push((e, true));
                false
            }
            Callback::StorageEnter3 => {
                self.storage_changes.push((e, false));
                false
            }
            Callback::PastureEnter2 => {
                self.pasture_enter2(e);
                false
            }
            Callback::PastureUpdate2 => {
                self.pasture_update2(jobs, e);
                true
            }
            Callback::PastureEnter3 => {
                self.pasture_leave(e);
                false
            }
            Callback::WorkPlaceEnter0 => {
                self.become_site(jobs, e);
                false
            }
            Callback::WorkPlaceEnter2 => {
                self.become_building(jobs, e);
                false
            }
            // `FUN_140134230`: every worker leaves and the workplace leaves its profession's.
            Callback::WorkPlaceEnter3 => {
                self.refresh_demolishing();
                let mut workers = self.workers();
                self.workforce.clear_workers(&mut workers, e);
                self.set_workers(&workers);
                if let Some(p) = self.workforce.workplaces.get(&e).and_then(|w| w.profession) {
                    self.workforce.unregister(e, p);
                }
                false
            }
            Callback::BuildEnter1
            | Callback::BuildUpdate1
            | Callback::BuildEnter8
            | Callback::BuildUpdate8
            | Callback::BuildExit8
            | Callback::BuildFinish
            | Callback::ResidenceEnter2
            | Callback::ResidenceEnter3 => self.call_building(jobs, e, f),
            Callback::DestroyEnter3 => {
                self.destroy_enter3(jobs, e);
                false
            }
            Callback::DestroyEnter4 => {
                self.destroy_enter4(jobs, e);
                false
            }
            Callback::DestroyUpdate4 => {
                self.destroy_update4(jobs, e);
                true
            }
            Callback::DestroyEnter5 => {
                self.destroy_enter5(jobs, e);
                true
            }
            Callback::DestroyUpdate5 => {
                self.destroy_update5(jobs, e);
                true
            }
            Callback::DestroyEnter6 => {
                self.destroy_enter6(e);
                false
            }
            Callback::DestroyUpdate6 => {
                self.destroy_update6(e, self.site_dt);
                true
            }
            Callback::DestroyExit6 => {
                self.destroy_exit6(e);
                true
            }
            Callback::ImmigrationEnter2 => {
                self.immigration_idle(e);
                false
            }
            Callback::ImmigrationUpdate2 => {
                self.immigration_idle(e);
                true
            }
            Callback::ImmigrationEnterF => {
                self.immigration_wait(e);
                false
            }
            Callback::ImmigrationUpdateF => {
                self.immigration_waiting(e, self.site_dt);
                true
            }
            Callback::ImmigrationEnter10 => {
                self.immigration_call(e, true);
                true
            }
            Callback::ImmigrationUpdate10 => {
                self.immigration_call(e, false);
                true
            }
            Callback::ImmigrationEnter11 => {
                self.band_arrived(e);
                true
            }
            Callback::ImmigrationUpdate11 => {
                self.band_waiting(e);
                true
            }
            Callback::ImmigrationLeave => {
                self.send_band_away(e);
                true
            }
            Callback::TradeEnter2 => {
                self.trade_enter2(jobs, e);
                false
            }
            Callback::TradeUpdate2 => {
                self.trade_update2(e, self.site_dt);
                false
            }
            Callback::TradeEnter12 => true,
            Callback::TradeEnter13 => {
                self.trade_enter13(e);
                true
            }
            Callback::TradeExit13 => {
                self.trade_exit13(e);
                true
            }
            Callback::TradeUpdate13 => {
                self.trade_update13(e, self.site_dt);
                true
            }
            Callback::TradeLeave => {
                self.trade_leave(e);
                true
            }
            Callback::MultiObjectEnter2 => {
                // `FUN_14012aa30`: each part's MapComponent activates (its minimap rectangle,
                // drawn by the town from the placed parts) and its floor is added
                // (`FUN_140124860`, which takes a floor already added out first): a bridge
                // piece's again, a dock's or porch's for the first time.
                let size = self.size;
                for part in self.sites[&e].parts.clone() {
                    let p = &self.placed[part];
                    if let Some(mesh) = self.templates[p.template].floor.clone() {
                        let floor = crate::world::floor::Floor::placed(&mesh, p);
                        self.cells.floors.add(size, part, floor);
                    }
                }
                false
            }
            Callback::WellEnter2 => {
                self.well_join(e, true);
                false
            }
            Callback::WellEnter3 => {
                self.well_join(e, false);
                false
            }
            Callback::AttendanceEnter2 => {
                self.attendance_enter(jobs, e);
                false
            }
            Callback::AttendanceExit2 => {
                // `FUN_1401318c0(icon, 2, full)`.
                let full = self.attendance_full(e);
                self.set_icon(e, crate::work::professions::icon::LIMIT, full);
                false
            }
            Callback::AttendanceEnter3 => {
                self.remove_work(jobs, e);
                self.attendance_leave(e);
                false
            }
            Callback::HealerEnter2 => {
                if !self.healers.contains(&e) {
                    self.healers.push(e);
                }
                false
            }
            Callback::HappinessEnter => {
                self.happiness_enter(e);
                false
            }
            Callback::HappinessUpdate => {
                self.happiness_update(e);
                false
            }
            Callback::HappinessLeave => {
                self.happiness_leave(e);
                false
            }
            Callback::HealerEnter3 => {
                self.healers.retain(|&x| x != e);
                false
            }
            Callback::ProduceEnter2 => {
                let off = self.sites[&e].produce.as_ref().is_some_and(|p| p.disabled);
                self.machine(e).request(if off { 0xe } else { 0xd });
                false
            }
            Callback::ProduceClearPoints => {
                self.produce_clear_points(e);
                false
            }
            Callback::ProduceEnterD => {
                if self.sites[&e].produce.as_ref().is_some_and(|p| p.disabled) {
                    self.machine(e).request(0xe);
                    return true;
                }
                // `FUN_140134130(work, 1, 0, …)` with slot `+0x90`'s work, then
                // `FUN_1401350d0(1)`.
                let t = &self.site_templates[self.sites[&e].template];
                let work = if t.herbalist.is_some() {
                    self.herbalist_work(e)
                } else if t.forester.is_some() {
                    self.forester_work(e)
                } else if t.fishing.is_some() {
                    self.fishing_work(e)
                } else if t.hunt.is_some() {
                    self.hunt_work(e)
                } else if t.market.is_some() {
                    self.market_work(e)
                } else {
                    self.produce_work(e)
                };
                self.set_work(jobs, e, work, 1, false);
                self.enable_workplace(e, true);
                // Slot `+0x98`.
                self.fishing_register(e);
                false
            }
            Callback::ProduceEnterE => {
                // `FUN_140134020` when registered, then `FUN_1401350d0(0)`.
                let id = self.sites[&e].work_id;
                if id >= 0 && jobs.works.contains_key(&id) {
                    self.remove_work(jobs, e);
                }
                self.enable_workplace(e, false);
                // Slot `+0xa0`.
                self.fishing_unregister(e);
                true
            }
            Callback::ProduceUpdateD => {
                // Slot `+0xa8`.
                self.fishing_water(e);
                false
            }
            Callback::ProduceLeaveD => {
                // Slot `+0xa0`.
                self.fishing_unregister(e);
                true
            }
            Callback::ClearResourceEnter2 => {
                // `FUN_1400c8370` flags the entity (`+0x40` bit 10) and queues it for the entity
                // manager's removal.
                if let Some(site) = self.sites.get_mut(&e) {
                    site.destroyed = true;
                }
                true
            }
        }
    }
}

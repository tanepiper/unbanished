//! The buildings' window buttons (ControlUI): a ControlUIConfig names a component (its type id) and
//! a control; the button shows the component's slot (bit 0 enabled, bit 1 on) and a click calls its
//! slot. See docs/BUILDING-WINDOWS.md.
//!
//! Original: `FUN_140060310`, `FUN_1400601d0`, `FUN_140060150`.
// +0x24; +0x20; +0x68; +0x70

use crate::town::Sim;
use crate::work::construction::{run, Callbacks};
use crate::work::jobs::Jobs;

/// The components with controls, by type id (`CRC32` of the UTF-16 class name).
pub mod kind {
    /// A crop field.
    pub const CROP_FIELD: u32 = 0xa3cc_d302;
    /// An orchard.
    pub const ORCHARD: u32 = 0xa981_9b71;
    /// A pasture.
    pub const PASTURE: u32 = 0x66ae_21ad;
    /// A trading post.
    pub const TRADE: u32 = 0x9292_c700;
    /// A school, chapel or doctor.
    pub const ATTENDANCE: u32 = 0x5146_364d;
    /// A forester's lodge.
    pub const FORESTER: u32 = 0xb18e_acda;
    /// ResourceProduceComponent and the classes that keep its slots.
    pub const CONSUME_PRODUCE: u32 = 0x1935_bdbb;
    /// A fishing dock.
    pub const FISHING: u32 = 0x9d9a_f271;
    /// A hunter's lodge.
    pub const HUNT: u32 = 0xacc2_7fa4;
    /// A market.
    pub const MARKET: u32 = 0x4c8f_7637;
    /// A gatherer's shelter or a herbalist.
    pub const HERBALIST: u32 = 0xca1a_33a3;
}

impl Sim<'_, '_> {
    // +0x70
    /// Slot of the `kind` component of `e`, control `index`.
    pub fn control(&mut self, jobs: &mut Jobs, e: usize, kind: u32, index: i32) {
        match kind {
            kind::CROP_FIELD => self.field_action(e, index),
            kind::ORCHARD => self.orchard_action(e, index),
            // `FUN_14012cf30`: only when the control is available.
            kind::PASTURE => {
                if self.pasture_query(e, index) & 1 != 0 {
                    self.pasture_action(jobs, e, index);
                }
            }
            kind::TRADE => self.toggle_trade_work(jobs, e),
            // `FUN_1400447e0`: the workplace on or off; switched off, everyone leaves
            // (`FUN_1400448c0`).
            kind::ATTENDANCE => {
                let on = !self.workforce.workplaces.get(&e).is_some_and(|w| w.enabled);
                self.enable_workplace(e, on);
                if !on {
                    self.evict(e);
                }
            }
            kind::FORESTER => self.forester_toggle(jobs, e, index),
            kind::CONSUME_PRODUCE | kind::FISHING | kind::HUNT | kind::MARKET | kind::HERBALIST => {
                self.produce_toggle(jobs, e)
            }
            _ => {}
        }
    }

    /// `FUN_140130a70`: the machine's message 0 (`FUN_1401302c0` in 0xd: go idle;
    /// `FUN_140130180` in 0xe: produce again unless switched off, `+0x48`), then a run.
    pub(crate) fn produce_toggle(&mut self, jobs: &mut Jobs, e: usize) {
        let Some(site) = self.sites.get(&e) else {
            return;
        };
        if site.machine.index < 0 {
            run(self, jobs, e, false, false);
        }
        let site = &self.sites[&e];
        let disabled = site.produce.as_ref().is_some_and(|p| p.disabled);
        match site.machine.state {
            0xd => self.machine(e).request(0xe),
            0xe if !disabled => self.machine(e).request(0xd),
            _ => {}
        }
        run(self, jobs, e, false, false);
    }

    /// `FUN_140124d60`: control 1 switches planting, 2 cutting; the first switched on or the
    /// last switched off toggles production (`FUN_140130a70`).
    fn forester_toggle(&mut self, jobs: &mut Jobs, e: usize, index: i32) {
        let Some(f) = self.sites.get_mut(&e).and_then(|s| s.forester.as_mut()) else {
            return;
        };
        let before = f.plant || f.cut;
        match index {
            1 => f.plant = !f.plant,
            2 => f.cut = !f.cut,
            _ => {}
        }
        let after = f.plant || f.cut;
        if before != after {
            self.produce_toggle(jobs, e);
        }
    }
}

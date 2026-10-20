//! Construction sites: the building state machine (StateMachineComponent with the ClearArea,
//! WorkPlace, Build and Residence controllers) and the construction works (`GatherRawMaterials`,
//! `BuildStructure`). See docs/JOBS.md.
//!
//! A placed building's controllers are linked in component order; a state is handled by the last
//! controller that defines it, and a callback returning false passes it to the previous one. The
//! callbacks themselves live on [`crate::town::Sim`].
//!
//! Original: `FUN_1400aeb30`.

mod machine;
mod site;
mod states;
pub use machine::{run, Machine};
pub use site::{
    Attendance, BuildWork, ClearWork, Construction, Decal, Gather, PickupWork, Pile, ProduceState,
    ProduceTemplate, ProduceWork, Recipe, Site, SiteTemplate,
};
pub use states::StateDef;

use std::collections::{BTreeMap, BTreeSet};

use crate::work::jobs::Jobs;
use crate::work::professions::{Command, Professions, Workforce};

/// The ControllerComponents the port has state tables for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Controller {
    /// Clears the building's area before construction.
    ///
    /// Original: `FUN_14011d4e0`.
    ClearArea,
    /// The workplace: workers and the work while the building works.
    ///
    /// Original: `FUN_140134870`.
    WorkPlace,
    /// Construction: materials, building work and the stages.
    ///
    /// Original: `FUN_14010e560`.
    Build,
    /// A home.
    ///
    /// Original: `FUN_14012f820`.
    Residence,
    /// ClearResourceComponent: the removal marker.
    ///
    /// Original: `FUN_140032150`, `FUN_140032270`.
    ClearResource,
    /// ResourceProduceComponent, here ConsumeProduceComponent.
    ///
    /// Original: `FUN_140130700`.
    ResourceProduce,
    /// AttendanceComponent: school, chapel, doctor.
    ///
    /// Original: `FUN_140044f90`.
    Attendance,
    /// HealerComponent: the herbalist.
    ///
    /// Original: `FUN_14004b980`.
    Healer,
    /// HappinessComponent.
    ///
    /// Original: `FUN_14004b570`.
    Happiness,
    /// CropFieldComponent.
    ///
    /// Original: `FUN_140120620`.
    CropField,
    /// OrchardComponent.
    ///
    /// Original: `FUN_140052a40`.
    Orchard,
    /// PastureComponent.
    ///
    /// Original: `FUN_14012d660`.
    Pasture,
    /// AbsorbResourceComponent: a storage yard takes in the piles on its footprint.
    ///
    /// Original: `FUN_14002f5d0`.
    AbsorbResource,
    /// StorageLocationComponent: a town storage.
    ///
    /// Original: `FUN_1401328e0`.
    StorageLocation,
    /// CemeteryComponent.
    ///
    /// Original: `FUN_140048100`.
    Cemetery,
    /// MarketLocationComponent: a town market.
    ///
    /// Original: `FUN_14004fa70`.
    MarketLocation,
    /// DestroyComponent: demolition, damage and rubble.
    ///
    /// Original: `FUN_1401231d0`.
    Destroy,
    /// WellComponent: a water source for firefighters.
    ///
    /// Original: `FUN_14005da70`.
    Well,
    /// ImmigrationComponent: the town hall's nomads.
    ///
    /// Original: `FUN_14004f020`.
    Immigration,
    /// TradeComponent: the trading post's merchants.
    ///
    /// Original: `FUN_14005cfb0`.
    Trade,
    /// MultiObjectComponent: a bridge's or a pasture's parts.
    ///
    /// Original: `FUN_14012b000`.
    MultiObject,
}

// exit +0x20; DAT_1401dbca0; +8; +0x18
/// A state callback (records: enter, scheduled update, transition functions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Callback {
    /// No callback.
    None,
    /// The clear-area controller: entering states 0 and 21.
    ///
    /// Original: `FUN_14011ce30`, `FUN_14011cdc0`, `FUN_14011cd60`.
    ClearEnter0,
    /// The clear-area controller: the scheduled update of states 0 and 21.
    ClearUpdate0,
    /// The clear-area controller: leaving state 0.
    ClearExit0,
    /// The clear-area controller: leaving state 21.
    ///
    /// Original: `FUN_14011cc40` (state 0x15 exit).
    ClearExit15,
    /// The clear-area controller: entering state 1.
    ///
    /// Original: `FUN_14011ccf0`, `FUN_14011ccc0`.
    ClearEnter1,
    /// The clear-area controller: entering state 2.
    ClearEnter2,
    /// The workplace controller: entering state 0.
    ///
    /// Original: `FUN_140134310` → `FUN_1401349f0`; `FUN_1401342c0`; `FUN_140134230`.
    WorkPlaceEnter0,
    /// The workplace controller: entering state 2.
    WorkPlaceEnter2,
    /// The workplace controller: entering state 3.
    WorkPlaceEnter3,
    /// The build controller: entering state 1.
    ///
    /// Original: `FUN_14010d760`, `FUN_14010d6b0`.
    BuildEnter1,
    /// The build controller: the scheduled update of state 1.
    BuildUpdate1,
    /// The build controller: entering state 8.
    ///
    /// Original: `FUN_14010cfb0`, `FUN_14010cbb0`, `FUN_14010cad0`.
    BuildEnter8,
    /// The build controller: the scheduled update of state 8.
    BuildUpdate8,
    /// The build controller: leaving state 8.
    BuildExit8,
    /// The build controller: the transition from state 8 to 2, when construction ends.
    ///
    /// Original: `FUN_14010cab0` → `FUN_14010d4c0`.
    BuildFinish,
    /// The residence: entering state 2.
    ///
    /// Original: `FUN_14012f2b0`.
    ResidenceEnter2,
    /// The marker destroys its entity once its cell is clear.
    ///
    /// Original: `FUN_14011da30`.
    ClearResourceEnter2,
    /// Built → producing (0xd), or idle (0xe) when switched off.
    ///
    /// Original: `FUN_140130680`.
    ProduceEnter2,
    /// The transition from state 2 to 0xd: the cells of the output points become empty ground.
    ///
    /// Original: `FUN_140130460`.
    ProduceClearPoints,
    /// Register the production work and enable the workplace (0xe when switched off).
    ///
    /// Original: `FUN_140130330`.
    ProduceEnterD,
    /// Remove the work and disable the workplace.
    ///
    /// Original: `FUN_1401301f0`.
    ProduceEnterE,
    // +0xa8
    /// State 0xd's scheduled update → slot (the dock's water).
    ///
    /// Original: `FUN_140130660`.
    ProduceUpdateD,
    // +0xa0
    /// Transition 0xd → 3 → slot (the dock leaves the map layer).
    ///
    /// Original: `FUN_140130440`.
    ProduceLeaveD,
    /// Register the attendance, its LocationWork, enable the workplace.
    ///
    /// Original: `FUN_1400445f0`.
    AttendanceEnter2,
    /// Status icon 2 when every attendance of the kind is full.
    ///
    /// Original: `FUN_140044590`.
    AttendanceExit2,
    /// Unregister, send the attendees away, disable the workplace.
    ///
    /// Original: `FUN_140044530`.
    AttendanceEnter3,
    /// Register / unregister the healer.
    ///
    /// Original: `FUN_14004b790`, `FUN_14004b750`.
    HealerEnter2,
    /// A healer: entering state 3.
    HealerEnter3,
    /// A happiness building: entering its active states.
    ///
    /// Original: `FUN_14004b350` (states 2 and 0xd enter), `FUN_14004b300` (their scheduled
    /// update), `FUN_14004b2a0` (state 3 enter).
    HappinessEnter,
    /// A happiness building: the scheduled update of its active states.
    HappinessUpdate,
    /// A happiness building: leaving its active states.
    HappinessLeave,
    /// A crop field: entering state 2.
    ///
    /// Original: `FUN_14011fdf0` / `FUN_14011fd30` (state 2), `FUN_14011f7e0` / `FUN_14011f6b0`
    /// (9), `FUN_14011f5d0` / `FUN_14011f440` / `FUN_14011f4a0` (10), `FUN_14011f330` /
    /// `FUN_14011f250` (11).
    CropEnter2,
    /// A crop field: the scheduled update of state 2.
    CropUpdate2,
    /// A crop field: entering state 9.
    CropEnter9,
    /// A crop field: the scheduled update of state 9.
    CropUpdate9,
    /// A crop field: entering state 10.
    CropEnter10,
    /// A crop field: leaving state 10.
    CropExit10,
    /// A crop field: the scheduled update of state 10.
    CropUpdate10,
    /// A crop field: entering state 11.
    CropEnter11,
    /// A crop field: the scheduled update of state 11.
    CropUpdate11,
    // +0x10
    /// The market joins / leaves the town's.
    ///
    /// Original: `FUN_14004f880`, `FUN_14004f840`.
    MarketEnter2,
    /// A market: entering state 3.
    MarketEnter3,
    /// The cemetery joins / leaves the town's.
    ///
    /// Original: `FUN_140047920`, `FUN_1400478e0`.
    CemeteryEnter2,
    /// A cemetery: entering state 3.
    CemeteryEnter3,
    /// AbsorbResource's state 0 update.
    ///
    /// Original: `FUN_14010ae20`.
    AbsorbUpdate0,
    /// Join / leave the town's storages.
    ///
    /// Original: `FUN_140132700`, `FUN_1401326c0`.
    StorageEnter2,
    /// A storage: entering state 3.
    StorageEnter3,
    /// A pasture: entering state 2.
    ///
    /// Original: `FUN_14012c590` / `FUN_14012bcb0` (state 2), `FUN_14012c310` (3).
    PastureEnter2,
    /// A pasture: the scheduled update of state 2.
    PastureUpdate2,
    /// A pasture: entering state 3.
    PastureEnter3,
    /// An orchard: entering state 2.
    ///
    /// Original: `FUN_140051e90` / `FUN_140051e20` (2), `FUN_1400519d0` / `FUN_1400518f0` (9),
    /// `FUN_140051790` / `FUN_140051560` (10), `FUN_140051500` / `FUN_140051310` (exits of 10 and
    /// 12), `FUN_140051440` / `FUN_140051370` (12), `FUN_140051160` / `FUN_140050fb0` (11),
    /// `FUN_140050e40` / `FUN_140050d40` / `FUN_140050d90` (0x14).
    OrchardEnter2,
    /// An orchard: the scheduled update of state 2.
    OrchardUpdate2,
    /// An orchard: entering state 9.
    OrchardEnter9,
    /// An orchard: the scheduled update of state 9.
    OrchardUpdate9,
    /// An orchard: entering state 10.
    OrchardEnter10,
    /// An orchard: the scheduled update of state 10.
    OrchardUpdate10,
    /// An orchard: leaving states 10 and 12, when the work ends.
    OrchardExitWork,
    /// An orchard: entering state 12.
    OrchardEnter12,
    /// An orchard: the scheduled update of state 12.
    OrchardUpdate12,
    /// An orchard: entering state 11.
    OrchardEnter11,
    /// An orchard: the scheduled update of state 11.
    OrchardUpdate11,
    /// An orchard: entering state 20.
    OrchardEnter14,
    /// An orchard: leaving state 20.
    OrchardExit14,
    /// An orchard: the scheduled update of state 20.
    OrchardUpdate14,
    /// The families move out and the residence leaves the town's.
    ///
    /// Original: `FUN_14012edf0`.
    ResidenceEnter3,
    /// Demolition: entering state 3.
    ///
    /// Original: `FUN_140122c80` (3), `FUN_1401224a0` / `FUN_140122390` (4), `FUN_140122140` /
    /// `FUN_1401215f0` (5), `FUN_1401229a0` / `FUN_140122960` / `FUN_1401228d0` (6).
    DestroyEnter3,
    /// Demolition: entering state 4.
    DestroyEnter4,
    /// Demolition: the scheduled update of state 4.
    DestroyUpdate4,
    /// Demolition: entering state 5.
    DestroyEnter5,
    /// Demolition: the scheduled update of state 5.
    DestroyUpdate5,
    /// Demolition: entering state 6.
    DestroyEnter6,
    /// Demolition: the scheduled update of state 6.
    DestroyUpdate6,
    /// Demolition: leaving state 6.
    DestroyExit6,
    /// The well joins / leaves the town's water sources.
    ///
    /// Original: `FUN_14005d880`, `FUN_14005d840`.
    WellEnter2,
    /// The well leaves the town's water sources.
    WellEnter3,
    /// The town hall's immigration: entering state 2.
    ///
    /// Original: `FUN_14004dbf0` / `FUN_14004db80` (2), `FUN_14004db00` / `FUN_14004da30` (0xf),
    /// `FUN_14004d9f0` / `FUN_14004d9d0` (0x10), `FUN_14004d810` / `FUN_14004d760` (0x11),
    /// `FUN_14004d660` (0x10 and 0x11 → 3).
    ImmigrationEnter2,
    /// The town hall's immigration: the scheduled update of state 2.
    ImmigrationUpdate2,
    /// The town hall's immigration: entering state 15.
    ImmigrationEnterF,
    /// The town hall's immigration: the scheduled update of state 15.
    ImmigrationUpdateF,
    /// The town hall's immigration: entering state 16.
    ImmigrationEnter10,
    /// The town hall's immigration: the scheduled update of state 16.
    ImmigrationUpdate10,
    /// The town hall's immigration: entering state 17.
    ImmigrationEnter11,
    /// The town hall's immigration: the scheduled update of state 17.
    ImmigrationUpdate11,
    /// The town hall's immigration: leaving its active states.
    ImmigrationLeave,
    /// The trading post: entering state 2.
    ///
    /// Original: `FUN_14005b530` / `FUN_14005b3c0` (2), `FUN_14005b3b0` (0x12), `FUN_14005b160` /
    /// `FUN_14005b030` / `FUN_14005b0e0` (0x13), `FUN_14005bac0` (→ 3).
    TradeEnter2,
    /// The trading post: the scheduled update of state 2.
    TradeUpdate2,
    /// The trading post: entering state 18.
    TradeEnter12,
    /// The trading post: entering state 19.
    TradeEnter13,
    /// The trading post: leaving state 19.
    TradeExit13,
    /// The trading post: the scheduled update of state 19.
    TradeUpdate13,
    /// The trading post: leaving its active states.
    TradeLeave,
    /// Activate the parts' MapComponents and floors.
    ///
    /// Original: `FUN_14012aa30`.
    MultiObjectEnter2,
    /// Callbacks whose port is still missing (they pass the state on).
    Missing,
}

use Callback as C;

/// What a callback needs to run, borrowed from the town (see [`crate::town::Sim`]).
pub trait Callbacks {
    /// Run a callback. Returns false to pass the state on to the previous controller.
    fn call(&mut self, jobs: &mut Jobs, site: usize, f: Callback) -> bool;
    /// A site's state machine.
    fn machine(&mut self, site: usize) -> &mut Machine;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Host {
        machine: Machine,
        log: Vec<Callback>,
        clear: bool,
    }

    impl Callbacks for Host {
        fn call(&mut self, _: &mut Jobs, _: usize, f: Callback) -> bool {
            if f == C::None {
                return false;
            }
            self.log.push(f);
            match f {
                C::ClearEnter0 | C::ClearUpdate0 => {
                    if self.clear {
                        self.machine.request(1);
                    }
                    f == C::ClearEnter0
                }
                C::BuildEnter1 => {
                    self.machine.request(8);
                    true
                }
                C::BuildEnter8 => {
                    self.machine.request(2);
                    true
                }
                C::WorkPlaceEnter0 | C::WorkPlaceEnter2 | C::ResidenceEnter2 => false,
                _ => true,
            }
        }
        fn machine(&mut self, _: usize) -> &mut Machine {
            &mut self.machine
        }
    }

    #[test]
    fn house_states_run_in_order_through_the_chain() {
        use Controller::*;
        let mut h = Host {
            machine: Machine::new(vec![ClearArea, WorkPlace, Build, Residence]),
            log: Vec::new(),
            clear: false,
        };
        let mut jobs = Jobs::default();
        run(&mut h, &mut jobs, 0, false, false);
        assert_eq!(h.log, vec![C::WorkPlaceEnter0, C::ClearEnter0]);
        assert_eq!(h.machine.state, 0);
        h.clear = true;
        h.log.clear();
        run(&mut h, &mut jobs, 0, true, true);
        assert_eq!(
            h.log,
            vec![
                C::ClearUpdate0,
                C::ClearExit0,
                C::BuildEnter1,
                C::BuildEnter8,
                C::BuildExit8,
                C::BuildFinish,
                C::ResidenceEnter2,
                C::WorkPlaceEnter2,
                C::ClearEnter2,
            ]
        );
        assert_eq!(h.machine.state, 2);
    }
}

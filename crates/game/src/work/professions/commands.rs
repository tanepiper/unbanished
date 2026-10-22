//! The player's commands on professions and workplaces, and applying them.

use super::*;

/// A player action on professions, applied by the simulation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    /// ProfessionUI spinner `300 + profession` set to `count`.
    ///
    /// Original: `FUN_14007b890`.
    SetCount {
        /// The profession.
        profession: usize,
        /// The number of workers wanted.
        count: i32,
    },
    /// WorkPlaceUI `0x1450`: one more worker of the workplace's profession.
    ///
    /// Original: `FUN_1400918a0`.
    AddWorker {
        /// The workplace.
        entity: usize,
    },
    /// WorkPlaceUI `0x1451`: one fewer.
    ///
    /// Original: `FUN_140096650`.
    RemoveWorker {
        /// The workplace.
        entity: usize,
    },
    /// WorkPlaceUI `0x1452/0x1453`: most workers ± 1.
    ///
    /// Original: `FUN_14008cae0`, `FUN_140134fd0`.
    ChangeMaxWorkers {
        /// The workplace.
        entity: usize,
        /// The change: +1 or −1.
        delta: i32,
    },
    /// The pause button of a building's window (element 6000).
    ///
    /// Original: `FUN_1400b7370`, `FUN_1401350d0`.
    SetEnabled {
        /// The building.
        entity: usize,
        /// Whether it works.
        enabled: bool,
    },
    // slot +0x58
    /// CropUI's crop list: the field's crop; choosing the first one switches farming on.
    ///
    /// Original: `FUN_1400b7df0`.
    SetCrop {
        /// The field.
        entity: usize,
        /// The crop, or none.
        crop: Option<usize>,
    },
    /// The field's buttons: 0 plant now, 1 harvest now, 2 farming on/off.
    ///
    /// Original: `FUN_140120190`.
    FieldAction {
        /// The field.
        entity: usize,
        /// The button: 0, 1 or 2.
        action: i32,
    },
    // slot +0x48
    /// PastureUI's slider: the most animals the pasture keeps.
    ///
    /// Original: `FUN_1400ba2f0`.
    SetLimit {
        /// The pasture.
        entity: usize,
        /// The most animals.
        limit: i32,
    },
    /// Demolish a building (the Remove Buildings tool, or the button in its window).
    ///
    /// Original: `FUN_140109b40`, `FUN_140123470`.
    Demolish {
        /// The building.
        entity: usize,
    },
    /// DestroyUI `0x1771`: the demolition is called off.
    ///
    /// Original: `FUN_1401238d0`.
    CancelDemolition {
        /// The building.
        entity: usize,
    },
    /// RoadTool's apply: plan a road on a cell.
    ///
    /// Original: `FUN_14010a200`, `FUN_1400a1e30`.
    PlanRoad {
        /// The cell.
        cell: i32,
        /// The road kind.
        kind: u8,
        /// The cell type of the road.
        ty: u8,
        /// The raw material it is built from, if any.
        material: Option<usize>,
    },
    /// Remove Roads: take a planned road back; a built one gets a removal marker (`marker`, the
    /// tool's template).
    ///
    /// Original: `FUN_140108ab0`, `FUN_1400a1bd0`.
    ClearRoad {
        /// The cell.
        cell: i32,
        /// The removal marker's template.
        marker: usize,
    },
    // slot +0x10
    /// A StateMachine message: DestroyComponent's Remove Cancel (0 while torn down), DestroyUI
    /// `0x1772` (0: rebuild the rubble) and `0x1773` (1: clear it); ImmigrationUI `0x9858` (7: take
    /// the nomads in) and `0x9859` (8: turn them away).
    ///
    /// Original: `FUN_140123940`, `FUN_1401239a0`.
    StateMessage {
        /// The entity.
        entity: usize,
        /// The message.
        message: i32,
    },
    // slot +0x70
    /// ControlUI: the `kind` component's control `index`.
    ///
    /// Original: `FUN_140060150`.
    Control {
        /// The entity.
        entity: usize,
        /// The component kind.
        kind: u32,
        /// The control's index.
        index: i32,
    },
    // +0x50
    /// ConsumeProduceUI's product: ConsumeProduceComponent.
    ///
    /// Original: `FUN_14005fc90`.
    SetRecipe {
        /// The workplace.
        entity: usize,
        /// The product.
        recipe: i32,
    },
    // +0x60
    /// Upgrade a building to `template` (the upgrade button): it is demolished, and the new
    /// building takes its place.
    ///
    /// Original: `FUN_14008c160`, then `FUN_140123470`.
    Upgrade {
        /// The building.
        entity: usize,
        /// The template of the new building.
        template: usize,
    },
    /// TradeUI's trade button: sell (material, count) and buy (trade good, count).
    ///
    /// Original: `FUN_1400868f0`.
    Exchange {
        /// The trading post.
        entity: usize,
        /// What the town sells: `(raw material, count)`.
        sell: Vec<(usize, i32)>,
        /// What the town buys: `(trade good, count)`.
        buy: Vec<(usize, i32)>,
    },
    /// TradeUI's settings on the TradeComponent.
    Trade {
        /// The trading post.
        entity: usize,
        /// The setting.
        setting: crate::commerce::trade::TradeSetting,
    },
}

impl Workforce {
    /// Apply a UI command.
    pub fn apply(
        &mut self,
        command: Command,
        workers: &mut [Worker],
        professions: &Professions,
        homes: &BTreeMap<usize, Home>,
        map: &mut dyn StorageSearchMap,
        search: &mut dyn Search,
    ) {
        match command {
            Command::SetCount { profession, count } => self.set_count(
                workers,
                professions,
                profession,
                count,
                homes,
                map,
                search.rng(),
            ),
            Command::AddWorker { entity } => {
                let Some(p) = self.workplaces.get(&entity).and_then(|w| w.profession) else {
                    return;
                };
                let wp = self.free_workplace(p, search.rng());
                self.hire(
                    workers,
                    professions.laborer,
                    p,
                    wp,
                    homes,
                    map,
                    search.rng(),
                );
            }
            Command::RemoveWorker { entity } => {
                if let Some(p) = self.workplaces.get(&entity).and_then(|w| w.profession) {
                    self.fire(workers, professions.laborer, p);
                }
            }
            // The simulation answers this one with the building's state
            // ([`crate::town::Sim::enable_workplace`]).
            Command::SetEnabled { entity, enabled } => {
                self.set_enabled(workers, entity, enabled, false, search)
            }
            Command::ChangeMaxWorkers { entity, delta } => {
                if let Some(w) = self.workplaces.get(&entity) {
                    let max = w.max() + delta;
                    self.set_max_workers(workers, entity, max, search);
                }
            }
            // Handled by the crop field, orchard and pasture (`crate::production::farm`, `crate::production::pasture`).
            Command::SetCrop { .. }
            | Command::FieldAction { .. }
            | Command::SetLimit { .. }
            | Command::Demolish { .. }
            | Command::CancelDemolition { .. }
            | Command::StateMessage { .. }
            | Command::PlanRoad { .. }
            | Command::ClearRoad { .. }
            | Command::Control { .. }
            | Command::SetRecipe { .. }
            | Command::Upgrade { .. }
            | Command::Exchange { .. }
            | Command::Trade { .. } => {}
        }
    }
}

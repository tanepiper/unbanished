//! Entity-window controllers (`UI::DialogController` subclasses attached by UIDescription):
//! slot `+0x20` binds, `+0x38` refreshes, `+0x48` handles element events. See
//! docs/BUILDING-WINDOWS.md. Refresh is read-only: nothing here touches simulation state or the
//! game Random.
//!
//! This file holds the shared data, the page toggle and the [`Controller`] that binds a window
//! to its parts; each part lives in a topic file: [`homes`], [`town`], [`production`],
//! [`construction`], [`storage`], [`workplace`] and [`citizens`].

mod citizens;
mod construction;
mod homes;
mod production;
mod storage;
mod town;
mod workplace;
pub(crate) use citizens::{str_less, text_of};
pub use citizens::{CitizenInfo, CitizenUi, StatusIconUi};
pub use construction::{BuildInfo, BuildProgress, BuildUi, DestroyInfo, DestroyUi};
pub use homes::{ResidenceUi, UpgradeUi};
pub use production::{ConsumeProduceUi, CountUi, ProductionUi};
use storage::material_file;
pub use storage::{StorageCapacityUi, StorageUi};
#[cfg(test)]
use town::resource_limit_config;
use town::status_icon_config;
pub(crate) use town::{set_spinner, spinner_value};
pub use town::{ControlUi, ImmigrationUi, ResourceLimitUi};
pub use workplace::{WorkPlaceClick, WorkPlaceInfo, WorkPlaceUi};

use std::collections::BTreeMap;

use unbanished_formats::game::{raw_material_description, RawMaterialDescription};
use unbanished_formats::pkg::Archive;
use unbanished_game::work::stock::{RawMaterial, Storage};
use unbanished_ui::{Control, Kind, NodeId, Ui};

/// CRC32 (UTF-16LE) of the controller class names.
pub const STORAGE_UI: u32 = 0x6186b6a4;
pub const STORAGE_CAPACITY_UI: u32 = 0x3061173b;
pub const WORKPLACE_UI: u32 = 0xbdb788dc;
pub const STATUS_ICON_UI: u32 = 0xdf3a1bb2;
pub const CITIZEN_UI: u32 = 0x03e455a9;
pub const BUILD_UI: u32 = 0x293e60a1;
pub const DESTROY_UI: u32 = 0x2e56a4c8;
pub const RESOURCE_LIMIT_UI: u32 = 0xf77895ff;
pub const CONTROL_UI: u32 = 0xb998cb9f;
pub const PRODUCTION_UI: u32 = 0x3998642a;
pub const PAGE_TOGGLE_UI: u32 = 0xc93b6026;
pub const ATTENDANCE_UI: u32 = 0x1aff79f6;
pub const CEMETERY_UI: u32 = 0xa17245fb;
pub const CONSUME_PRODUCE_UI: u32 = 0x9c93d6c1;
pub const UPGRADE_UI: u32 = 0x061d5f73;
pub const IMMIGRATION_UI: u32 = 0xff9794df;
pub const RESIDENCE_UI: u32 = 0x1701e613;
pub const TOWN_HALL_UI: u32 = 0xcd3b5953;
pub const TRADE_UI: u32 = 0xc8b87610;
pub const CROP_UI: u32 = 0x111542c0;
pub const ORCHARD_UI: u32 = 0x33908208;
pub const PASTURE_UI: u32 = 0xae1a9e32;
/// `RadiusUI`: the base class's defaults (no bind, refresh or events).
pub const RADIUS_UI: u32 = 0x00585b87;

/// What a controller reads from the selected entity.
pub struct EntityData<'a> {
    pub storage: Option<&'a Storage>,
    pub materials: &'a [RawMaterial],
    /// StatusIconComponent `+0xf0`.
    pub status: u32,
    /// The building's WorkPlaceComponent, when it has a profession (`+0x48`).
    pub workplace: Option<WorkPlaceInfo>,
    /// The building's Build and ClearArea components (a construction site).
    pub build: Option<BuildInfo>,
    /// Host `pages` active page (StatusIconUI shows icons only on pages 2 and 3).
    pub active_page: usize,
    pub citizen: Option<CitizenInfo>,
    /// The building's DestroyComponent.
    pub destroy: Option<DestroyInfo>,
    /// The stock manager's limits by slot (`Game +0xf0 → +0x170`, `+0x8`).
    pub stock_limits: [i32; 21],
    /// The building's controls (slot `+0x68` of the component `kind`, control `index`).
    pub control: Option<&'a dyn Fn(u32, i32) -> u8>,
    /// The TrackerComponent's this year's and last year's (material, amount), the year
    /// already turned (`FUN_14007af20`).
    pub tracker: Option<[Vec<(usize, i32)>; 2]>,
    /// AttendanceComponent: attendees (`+0x30`) and places (description `+0x20`).
    pub attendance: Option<(i32, i32)>,
    /// CemeteryComponent: graves with a tombstone (`FUN_140047730`) and graves (`+0x50`).
    pub cemetery: Option<(i32, i32)>,
    /// ConsumeProduceComponent: the entity, its template's reference, the selector (`+0x50`),
    /// productions made (`+0x68`) and the description's lifetime (`+0x48`).
    pub produce: Option<(usize, String, i32, i32, i32)>,
    /// ImmigrationComponent: nomads waiting at the town hall (state 0x11) and how many
    /// (`+0x60`); the town's name (`Game +0x28`).
    pub immigration: Option<(bool, i32)>,
    pub town_name: &'a str,
    /// ResidenceComponent: its residents, family by family (`+0x50`).
    pub residents: Option<Vec<(usize, CitizenInfo)>>,
    /// The town and the stock manager's history (TownHallUI).
    pub town: Option<&'a crate::start::StartingTown>,
    pub town_ctx: Option<&'a crate::ui::town_hall_ui::TownContext<'a>>,
    /// The selected building (TradeUI).
    pub entity: Option<usize>,
}

/// `PageToggleUI` (type `0xc93b6026`): bind `FUN_14007ad20`, refresh `FUN_14007ac50`, events
/// `FUN_14007abe0`. Its check button (id 700) switches the host's `pages` between the
/// working page (0) and the inventory (4), enabled while one of them shows.
pub struct PageToggleUi {
    button: Option<NodeId>,
    pages: Option<NodeId>,
}

fn active_page(ui: &Ui, pages: NodeId) -> Option<usize> {
    match &ui.node(pages).kind {
        Kind::Control(Control::Page { selected, .. }) => Some(*selected),
        _ => None,
    }
}

fn set_page(ui: &mut Ui, pages: NodeId, page: usize) {
    if let Kind::Control(Control::Page { selected, .. }) = &mut ui.node_mut(pages).kind {
        *selected = page;
    }
}

impl PageToggleUi {
    pub fn bind(ui: &mut Ui, element: NodeId, root: NodeId) -> PageToggleUi {
        let mut stack = vec![element];
        let mut button = None;
        while let Some(n) = stack.pop() {
            if matches!(ui.node(n).kind, Kind::Control(Control::Check { .. })) {
                button = Some(n);
                break;
            }
            stack.extend(ui.children(n).into_iter().rev());
        }
        // `+0x40`, the page it keeps, starts at the working page.
        let pages = ui.find(root, "pages");
        if let Some(p) = pages {
            if matches!(active_page(ui, p), Some(0) | Some(4)) {
                set_page(ui, p, 0);
            }
        }
        PageToggleUi { button, pages }
    }

    /// `FUN_14007ac50`: the button works on the working page and the inventory.
    pub fn refresh(&mut self, ui: &mut Ui) {
        let (Some(b), Some(p)) = (self.button, self.pages) else {
            return;
        };
        ui.node_mut(b).enabled = matches!(active_page(ui, p), Some(0) | Some(4));
    }

    /// `FUN_14007abe0`: checked shows the inventory (4), else the working page.
    pub fn click(&mut self, ui: &mut Ui, node: NodeId) -> bool {
        let (Some(b), Some(p)) = (self.button, self.pages) else {
            return false;
        };
        if node != b && !ui.descendant(b, node) {
            return false;
        }
        let page = if ui.checked(b) { 4 } else { 0 };
        set_page(ui, p, page);
        true
    }
}

pub enum Controller {
    Citizen(CitizenUi),
    Storage(StorageUi),
    Capacity(StorageCapacityUi),
    WorkPlace(WorkPlaceUi),
    Build(BuildUi),
    StatusIcon(StatusIconUi),
    Destroy(DestroyUi),
    ResourceLimit(ResourceLimitUi),
    Control(ControlUi),
    Production(ProductionUi),
    PageToggle(PageToggleUi),
    Count(CountUi),
    ConsumeProduce(ConsumeProduceUi),
    Upgrade(UpgradeUi),
    Immigration(ImmigrationUi),
    Residence(ResidenceUi),
    TownHall(crate::ui::town_hall_ui::TownHallUi),
    Trade(crate::ui::trade_ui::TradeUi),
    SelectResource(crate::ui::select_resource_ui::SelectResourceUi),
    /// `RadiusUI`, and any controller id the shipped data does not use (logged once with
    /// its id): attached, nothing to refresh.
    #[allow(dead_code)]
    Inert(u32),
}

/// `UI::TabElement`'s active tab.
pub(crate) fn tab_index(ui: &Ui, tabs: Option<NodeId>) -> i32 {
    match tabs.map(|t| &ui.node(t).kind) {
        Some(Kind::Control(Control::Tabs { selected, .. })) => *selected as i32,
        _ => 0,
    }
}

/// `UI::TabElement::SetActiveTab`.
pub(crate) fn set_tab(ui: &mut Ui, tabs: Option<NodeId>, index: i32) {
    let Some(t) = tabs else { return };
    if let Kind::Control(Control::Tabs { selected, tabs, .. }) = &mut ui.node_mut(t).kind {
        if (0..tabs.len() as i32).contains(&index) {
            *selected = index as usize;
        }
    }
}

impl Controller {
    /// Slot `+0x50` (`OnSerializeSettings`) writing: the controller's own settings block;
    /// every controller but TradeUI and TownHallUI keeps nothing.
    pub fn settings(&self, ui: &Ui) -> Vec<u8> {
        match self {
            Controller::Trade(c) => c.settings(ui),
            Controller::TownHall(c) => c.settings(ui),
            _ => Vec::new(),
        }
    }

    /// Slot `+0x50` reading.
    pub fn apply_settings(&mut self, ui: &mut Ui, settings: &[u8]) {
        match self {
            Controller::Trade(c) => c.apply_settings(ui, settings),
            Controller::TownHall(c) => c.apply_settings(ui, settings),
            _ => {}
        }
    }

    pub fn bind(
        ui: &mut Ui,
        archive: &Archive,
        type_id: u32,
        element: NodeId,
        root: NodeId,
        config: Option<&str>,
    ) -> Controller {
        let bound = match type_id {
            CITIZEN_UI => Some(Controller::Citizen(CitizenUi::bind(ui, element, root))),
            STORAGE_UI => StorageUi::bind(ui, element, archive).map(Controller::Storage),
            STORAGE_CAPACITY_UI => StorageCapacityUi::bind(ui, element).map(Controller::Capacity),
            WORKPLACE_UI => Some(Controller::WorkPlace(WorkPlaceUi::bind(ui, element))),
            BUILD_UI => Some(Controller::Build(BuildUi::bind(ui, element))),
            DESTROY_UI => Some(Controller::Destroy(DestroyUi::bind(ui, element))),
            RESOURCE_LIMIT_UI => Some(Controller::ResourceLimit(ResourceLimitUi::bind(
                ui, archive, element, config,
            ))),
            CONTROL_UI => ControlUi::bind(archive, element, config).map(Controller::Control),
            PRODUCTION_UI => Some(Controller::Production(ProductionUi::bind(ui, element))),
            PAGE_TOGGLE_UI => Some(Controller::PageToggle(PageToggleUi::bind(
                ui, element, root,
            ))),
            ATTENDANCE_UI => Some(Controller::Count(CountUi::bind(ui, element, false))),
            CEMETERY_UI => Some(Controller::Count(CountUi::bind(ui, element, true))),
            CONSUME_PRODUCE_UI => {
                Some(Controller::ConsumeProduce(ConsumeProduceUi::bind(ui, root)))
            }
            UPGRADE_UI => Some(Controller::Upgrade(UpgradeUi::bind(
                ui, archive, element, config,
            ))),
            IMMIGRATION_UI => Some(Controller::Immigration(ImmigrationUi::bind(ui, element))),
            RESIDENCE_UI => Some(Controller::Residence(ResidenceUi::bind(ui, element))),
            TOWN_HALL_UI => Some(Controller::TownHall(
                crate::ui::town_hall_ui::TownHallUi::bind(ui, archive, element),
            )),
            TRADE_UI => Some(Controller::Trade(crate::ui::trade_ui::TradeUi::bind(
                ui, element,
            ))),
            CROP_UI | ORCHARD_UI | PASTURE_UI => {
                use crate::ui::select_resource_ui::{Resource, SelectResourceUi};
                let resource = match type_id {
                    CROP_UI => Resource::Crop,
                    ORCHARD_UI => Resource::Orchard,
                    _ => Resource::Pasture,
                };
                Some(Controller::SelectResource(SelectResourceUi::bind(
                    ui, resource, element, root,
                )))
            }
            // Without a config, or with `_showOnMainPage`, the icons show on every page.
            STATUS_ICON_UI => {
                let always = config.is_none_or(|c| {
                    archive
                        .read_ref(&c.replace('\\', "/"))
                        .ok()
                        .and_then(|d| status_icon_config(&d))
                        .unwrap_or(false)
                });
                StatusIconUi::bind(ui, element, always).map(Controller::StatusIcon)
            }
            RADIUS_UI => Some(Controller::Inert(type_id)),
            _ => None,
        };
        bound.unwrap_or_else(|| {
            log::warn!("entity controller {type_id:#010x}: unknown");
            Controller::Inert(type_id)
        })
    }

    pub fn refresh(&mut self, ui: &mut Ui, archive: &Archive, data: &EntityData) {
        match self {
            Controller::Citizen(c) => c.refresh(ui, data),
            Controller::Storage(c) => c.refresh(ui, archive, data),
            Controller::Capacity(c) => c.refresh(ui, data),
            Controller::WorkPlace(c) => c.refresh(ui, data),
            Controller::Build(c) => c.refresh(ui, archive, data),
            Controller::StatusIcon(c) => c.refresh(ui, data),
            Controller::Destroy(c) => c.refresh(ui, data),
            Controller::ResourceLimit(c) => c.refresh(ui, data),
            Controller::Control(c) => c.refresh(ui, data),
            Controller::Production(c) => c.refresh(ui, archive, data),
            Controller::PageToggle(c) => c.refresh(ui),
            Controller::Count(c) => c.refresh(ui, data),
            Controller::ConsumeProduce(c) => c.refresh(ui, archive, data),
            Controller::Upgrade(_) => {}
            Controller::Immigration(c) => c.refresh(ui, data),
            Controller::Residence(c) => c.refresh(ui, data),
            Controller::TownHall(c) => {
                if let (Some(t), Some(x)) = (data.town, data.town_ctx) {
                    c.refresh(ui, archive, t, x);
                }
            }
            Controller::Trade(c) => {
                if let (Some(t), Some(e)) = (data.town, data.entity) {
                    c.refresh(ui, archive, t, e);
                }
            }
            Controller::SelectResource(c) => {
                if let (Some(t), Some(e)) = (data.town, data.entity) {
                    c.refresh(ui, archive, t, e);
                }
            }
            Controller::Inert(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The crop field's build-status icons keep to the status pages (`_showOnMainPage` false).
    #[test]
    fn status_icon_configs_read_their_flag() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(unbanished_formats::pkg::Package::parse(bytes).unwrap());
        }
        let d = archive
            .read_ref("Template/CropField.rsc:titleBuildStatus")
            .unwrap();
        assert_eq!(status_icon_config(&d), Some(false));
    }

    /// The shipped ResourceLimitUIConfigs name their stock slots (the cutter yard's firewood
    /// is slot 3).
    #[test]
    fn resource_limit_configs_name_slots() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(unbanished_formats::pkg::Package::parse(bytes).unwrap());
        }
        let slot = |t: &str| {
            archive
                .read_ref(&format!("Template/{t}.rsc:resourceLimitConfig"))
                .ok()
                .and_then(|d| resource_limit_config(&d))
        };
        assert_eq!(slot("CutterYard"), Some(3));
        assert!(slot("CropField").is_some());
    }

    #[test]
    fn signed_byte_order() {
        assert!(str_less("Coal", "Iron"));
        assert!(str_less("Log", "Logs"));
        assert!(!str_less("Logs", "Log"));
        // Bytes >= 0x80 compare as negative `char`.
        assert!(str_less("Ä", "A"));
    }
}

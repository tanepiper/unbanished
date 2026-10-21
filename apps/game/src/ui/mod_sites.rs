//! The Mods window's Browse tabs, one per mod site (the `mod-sites` extension,
//! [`crate::ui::mod_browse`]). In a build without it, or a Pure Banished run, the window has
//! none: the Browse tab and the Workshop progress are hidden, as in the original's build
//! without Steam.

use unbanished_ui::{NodeId, Ui};

use crate::mods::Modification;
use crate::settings::Persistence;
#[cfg(feature = "mod-sites")]
use crate::ui::mod_browse::Browse;

/// The Browse tabs of an open Mods window.
pub struct Sites {
    #[cfg(feature = "mod-sites")]
    tabs: Vec<Browse>,
}

impl Sites {
    /// Opens a Browse tab for each mod site in the Mods window `root`.
    pub fn open(
        ui: &mut Ui,
        root: NodeId,
        settings: &Persistence,
        pack_dir: Option<std::path::PathBuf>,
    ) -> Sites {
        #[cfg(feature = "mod-sites")]
        if !crate::platform::mode::pure() {
            return Sites {
                tabs: Browse::open_all(ui, root, settings, pack_dir),
            };
        }
        let _ = (settings, pack_dir);
        for name in ["buttonBrowse", "groupProgress"] {
            if let Some(n) = ui.find(root, name) {
                ui.set_visible(n, false);
            }
        }
        Sites {
            #[cfg(feature = "mod-sites")]
            tabs: Vec::new(),
        }
    }

    /// A click in a Browse tab: `Some(true)` to reload with the new mods, `Some(false)` when a
    /// tab took the click, `None` when none did.
    pub fn click(
        &mut self,
        ui: &mut Ui,
        settings: &Persistence,
        mods: &Modification,
        node: NodeId,
    ) -> Option<bool> {
        #[cfg(feature = "mod-sites")]
        for b in &mut self.tabs {
            if let Some(reload) = b.click(ui, settings, mods, node) {
                return Some(reload);
            }
        }
        #[cfg(not(feature = "mod-sites"))]
        let _ = (ui, settings, mods, node);
        None
    }

    /// A combo box's choice in a Browse tab.
    pub fn selected(
        &mut self,
        ui: &mut Ui,
        settings: &Persistence,
        mods: &Modification,
        node: NodeId,
        index: usize,
    ) {
        #[cfg(feature = "mod-sites")]
        for b in &mut self.tabs {
            if b.selected(ui, settings, mods, node, index) {
                break;
            }
        }
        #[cfg(not(feature = "mod-sites"))]
        let _ = (ui, settings, mods, node, index);
    }

    /// Each frame: the tabs' background replies. A finished install joins the list and is
    /// saved in the settings.
    pub fn poll(
        &mut self,
        ui: &mut Ui,
        settings: &mut crate::settings::Settings,
        mods: &mut Modification,
    ) {
        #[cfg(feature = "mod-sites")]
        for b in &mut self.tabs {
            let before = mods.mods.len();
            b.poll(ui, &mut settings.persistence, mods);
            if mods.mods.len() != before {
                settings.save();
            }
        }
        #[cfg(not(feature = "mod-sites"))]
        let _ = (ui, settings, mods);
    }
}

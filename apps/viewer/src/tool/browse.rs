//! The asset lists: the items of each mode, their folders, categories and display names.

use super::*;

impl Tool {
    pub(super) fn items(&self, mode: Mode) -> &[String] {
        match mode {
            Mode::Models => &self.assets.meshes,
            Mode::Textures => &self.assets.textures,
            Mode::Audio => &self.assets.sounds,
            Mode::Dialogs => &self.assets.dialogs,
        }
    }

    pub(super) fn resource_dirs(&self, item: &str) -> Vec<String> {
        let path = item.replace('\\', "/");
        if self.mode == Mode::Dialogs {
            let stem = path.trim_end_matches(".crs");
            let stem = stem.strip_prefix("Dialog/").unwrap_or(stem);
            let (group, object) = stem.rsplit_once('_').unwrap_or((stem, "resource"));
            return vec![group.to_owned(), object.to_owned()];
        }
        let mut parts: Vec<_> = path.split('/').collect();
        parts.pop(); // resource filename
        let Some(first) = parts.first().copied() else {
            return Vec::new();
        };
        let root: &[&str] = match self.mode {
            Mode::Models => &[
                "Models",
                "Model",
                "Meshes",
                "Mesh",
                "Graphics",
                "GraphicsMesh",
                "",
            ],
            Mode::Textures => &["Textures", "Texture", "Images", "ImageBuffer", ""],
            Mode::Audio => &["Audio", "Sounds", "Sound", "SoundBuffer", ""],
            Mode::Dialogs => &["Dialogs", "Dialog", "UI", ""],
        };
        let skip = root
            .iter()
            .any(|name| !name.is_empty() && first.eq_ignore_ascii_case(name));
        parts
            .into_iter()
            .skip(usize::from(skip))
            .map(str::to_owned)
            .collect()
    }

    pub(super) fn category_names(&self) -> Vec<String> {
        let categories: std::collections::BTreeSet<_> = self
            .items(self.mode)
            .iter()
            .filter_map(|item| self.resource_dirs(item).first().cloned())
            .collect();
        std::iter::once("All".to_owned())
            .chain(categories)
            .collect()
    }

    pub(super) fn subcategory_names(&self, category: usize) -> Vec<String> {
        let selected = self.category_names().get(category).cloned();
        let subcategories: std::collections::BTreeSet<_> = self
            .items(self.mode)
            .iter()
            .filter_map(|item| {
                let dirs = self.resource_dirs(item);
                if category > 0 && dirs.first() != selected.as_ref() {
                    return None;
                }
                dirs.get(1).cloned()
            })
            .collect();
        std::iter::once("All".to_owned())
            .chain(subcategories)
            .collect()
    }

    pub(super) fn filtered_items(&self, category: usize, subcategory: usize) -> Vec<String> {
        let category_name = self.category_names().get(category).cloned();
        let subcategory_name = self.subcategory_names(category).get(subcategory).cloned();
        if self.mode == Mode::Dialogs && category == 0 && subcategory == 0 {
            // The default dialog list represents each named dialog once. The
            // group and object selectors expose its individual resources.
            let mut groups = std::collections::BTreeMap::<String, Vec<String>>::new();
            for item in self.items(self.mode) {
                if let Some(group) = self.resource_dirs(item).first() {
                    groups.entry(group.clone()).or_default().push(item.clone());
                }
            }
            return groups
                .into_iter()
                .filter_map(|(group, mut items)| {
                    items.sort();
                    let preferred = match group.as_str() {
                        "Loading" => "_gameLoad0.",
                        // The generic SaveLoad resource combines the Save title
                        // with a Load action. Use its explicit Load dialog as
                        // the group's preview; the selector still lists every
                        // SaveLoad resource, including the generic resource.
                        "SaveLoad" => "_confirmLoad.",
                        _ => "_resource.",
                    };
                    let selected = items
                        .iter()
                        .position(|item| item.contains(preferred))
                        .unwrap_or(0);
                    items.get(selected).cloned()
                })
                .collect();
        }
        self.items(self.mode)
            .iter()
            .filter(|item| {
                let dirs = self.resource_dirs(item);
                (category == 0 || dirs.first() == category_name.as_ref())
                    && (subcategory == 0 || dirs.get(1) == subcategory_name.as_ref())
            })
            .cloned()
            .collect()
    }

    pub(super) fn display_name(entry: &str) -> String {
        entry.trim_end_matches(".crs").replace('\\', "/")
    }

    pub(super) fn display_resource_name(&self, entry: &str) -> String {
        let path = entry.replace('\\', "/");
        if self.mode == Mode::Dialogs {
            let stem = path.trim_end_matches(".crs");
            let stem = stem.strip_prefix("Dialog/").unwrap_or(stem);
            let (group, object) = stem.rsplit_once('_').unwrap_or((stem, "resource"));
            return if self.selected_category == 0 {
                group.to_owned()
            } else if self.selected_subcategory > 0 {
                format!("{group}_{object}")
            } else {
                object.to_owned()
            };
        }
        let mut parts: Vec<_> = path.split('/').collect();
        let dirs = self.resource_dirs(entry);
        let root_parts = parts.len().saturating_sub(dirs.len() + 1);
        let category_parts = usize::from(self.selected_category > 0).min(dirs.len());
        let subcategory_parts = usize::from(self.selected_subcategory > 0)
            .min(dirs.len().saturating_sub(category_parts));
        parts.drain(..root_parts + category_parts + subcategory_parts);
        parts.join("/").trim_end_matches(".crs").to_owned()
    }
}

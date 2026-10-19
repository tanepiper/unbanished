//! The interface's resources from the game's packages, each loaded once: sprite sheets, fonts,
//! element descriptions and string tables.

use std::collections::HashMap;
use std::rc::Rc;

use banished_formats::pkg::Archive;
use banished_formats::types;
use banished_formats::ui::{self, Element, Font, SpriteSheet};

/// Index of a sprite sheet in [`Resources::sheets`].
pub type SheetId = usize;
/// The index of a font in [`Resources::fonts`].
pub type FontId = usize;

#[derive(Debug, Clone, Copy, PartialEq)]
/// One sprite: its sheet, its size and its place on the sheet.
pub struct SpriteRef {
    /// The sheet.
    pub sheet: SheetId,
    /// The width in pixels.
    pub width: i32,
    /// The height in pixels.
    pub height: i32,
    /// The texture coordinates on the sheet: u0, v0, u1, v1.
    pub uv: [f32; 4],
}

/// A loaded sprite sheet.
pub struct LoadedSheet {
    /// The sheet's resource reference.
    pub reference: String,
    /// The sheet: its sprites by name.
    pub sheet: SpriteSheet,
    /// `ImageBuffer` reference holding the sheet image (the material's `diffuse` slot).
    pub image: Option<String>,
    /// The material instance's base material (e.g. `Material\UI\Logo.rsc:resource`), which
    /// selects the shader. Most sheets use the plain UI material.
    pub shader: Option<String>,
    /// An image made at run time (`width`, `height`, RGBA) instead of `image`: a save's
    /// preview, which the Save and Load dialogs wrap in a sheet of their own.
    pub pixels: Option<Rc<(u32, u32, Vec<u8>)>>,
}

/// A loaded font.
pub struct LoadedFont {
    /// The font: its glyphs and metrics.
    pub font: Font,
    /// The sprite sheet of its glyphs.
    pub sheet: Option<SheetId>,
    /// Pixels from the line top to the baseline.
    pub ascent: i32,
}

/// The resource cache.
pub struct Resources {
    /// The game's packages.
    pub archive: Rc<Archive>,
    elements: HashMap<String, Option<Rc<Element>>>,
    sheet_ids: HashMap<String, Option<SheetId>>,
    /// The loaded sprite sheets.
    pub sheets: Vec<LoadedSheet>,
    font_ids: HashMap<String, Option<FontId>>,
    /// The loaded fonts.
    pub fonts: Vec<LoadedFont>,
    tables: HashMap<String, Rc<Option<HashMap<String, String>>>>,
    /// The same tables in serialized order, for index lookups.
    ordered: HashMap<String, Rc<Vec<(String, String)>>>,
}

impl Resources {
    /// An empty cache over `archive`.
    pub fn new(archive: Rc<Archive>) -> Self {
        Resources {
            archive,
            elements: HashMap::new(),
            sheet_ids: HashMap::new(),
            sheets: Vec::new(),
            font_ids: HashMap::new(),
            fonts: Vec::new(),
            tables: HashMap::new(),
            ordered: HashMap::new(),
        }
    }

    /// An element description by reference.
    pub fn element(&mut self, reference: &str) -> Option<Rc<Element>> {
        if let Some(e) = self.elements.get(reference) {
            return e.clone();
        }
        let e = self
            .archive
            .read_ref(reference)
            .and_then(|d| ui::element(&d))
            .map_err(|e| log::warn!("{reference}: {e}"))
            .ok()
            .map(Rc::new);
        self.elements.insert(reference.to_string(), e.clone());
        e
    }

    /// A sprite sheet by reference.
    pub fn sheet(&mut self, reference: &str) -> Option<SheetId> {
        if let Some(id) = self.sheet_ids.get(reference) {
            return *id;
        }
        let id =
            self.archive
                .read_ref(reference)
                .and_then(|d| ui::sprite_sheet(&d))
                .map_err(|e| log::warn!("{reference}: {e}"))
                .ok()
                .map(|sheet| {
                    let mi = sheet.material.as_deref().and_then(|m| {
                        types::material_instance(&self.archive.read_ref(m).ok()?).ok()
                    });
                    let image = mi
                        .as_ref()
                        .and_then(|mi| mi.texture("diffuse").map(str::to_owned));
                    let shader = mi.and_then(|mi| mi.material);
                    self.sheets.push(LoadedSheet {
                        reference: reference.to_string(),
                        sheet,
                        image,
                        shader,
                        pixels: None,
                    });
                    self.sheets.len() - 1
                });
        self.sheet_ids.insert(reference.to_string(), id);
        id
    }

    /// A sheet with one sprite, around an image made at run time, for example a save's preview
    /// picture. `key` names it: the same key gives the same sheet, with the image it was first
    /// made with.
    ///
    /// Original: `FUN_1400bc1b0` (a `UI::SpriteSheet` with `AddSpriteInfo(0, 0, w, h)`).
    pub fn image_sheet(&mut self, key: &str, width: u32, height: u32, rgba: Vec<u8>) -> SpriteRef {
        let reference = format!("image:{key}");
        let sheet = match self.sheet_ids.get(&reference) {
            Some(Some(id)) => *id,
            _ => {
                self.sheets.push(LoadedSheet {
                    reference: reference.clone(),
                    sheet: SpriteSheet {
                        material_path: String::new(),
                        source_image: String::new(),
                        width: width as i32,
                        height: height as i32,
                        unknown_flag: false,
                        sprites: Vec::new(),
                        material: None,
                    },
                    image: None,
                    shader: None,
                    pixels: Some(Rc::new((width, height, rgba))),
                });
                let id = self.sheets.len() - 1;
                self.sheet_ids.insert(reference, Some(id));
                id
            }
        };
        SpriteRef {
            sheet,
            width: width as i32,
            height: height as i32,
            uv: [0.0, 0.0, 1.0, 1.0],
        }
    }

    /// A sprite by its sheet's reference and its name.
    pub fn sprite(&mut self, sheet_ref: Option<&str>, name: &str) -> Option<SpriteRef> {
        if name.is_empty() {
            return None;
        }
        let sheet = self.sheet(sheet_ref?)?;
        let s = self.sheets[sheet].sheet.sprite(name)?;
        Some(SpriteRef {
            sheet,
            width: s.width,
            height: s.height,
            uv: s.uv,
        })
    }

    /// A font by reference.
    pub fn font(&mut self, reference: &str) -> Option<FontId> {
        if let Some(id) = self.font_ids.get(reference) {
            return *id;
        }
        let id = self
            .archive
            .read_ref(reference)
            .and_then(|d| ui::font(&d))
            .map_err(|e| log::warn!("{reference}: {e}"))
            .ok()
            .map(|font| {
                let sheet = font.sprite_sheet.as_deref().and_then(|s| self.sheet(s));
                // TextElement::Draw uses the serialized descent, not the tallest bitmap.
                let ascent = font.baseline_offset();
                self.fonts.push(LoadedFont {
                    font,
                    sheet,
                    ascent,
                });
                self.fonts.len() - 1
            });
        self.font_ids.insert(reference.to_string(), id);
        id
    }

    /// `UI::String(table, index)`: entry `index` of a string table in serialized order
    /// (`StringTable::GetIndexFromName` searches that order). Empty when out of range.
    pub fn string_at(&mut self, table: &str, index: i32) -> String {
        let list = match self.ordered.get(table) {
            Some(l) => l.clone(),
            None => {
                let l = Rc::new(
                    self.archive
                        .read_ref(table)
                        .and_then(|d| ui::string_table(&d))
                        .unwrap_or_default(),
                );
                self.ordered.insert(table.to_string(), l.clone());
                l
            }
        };
        usize::try_from(index)
            .ok()
            .and_then(|i| list.get(i))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }

    /// Resolve a string-table key; falls back to the key itself.
    pub fn string(&mut self, table: Option<&str>, key: &str) -> String {
        let Some(table) = table else {
            return key.to_string();
        };
        let map = match self.tables.get(table) {
            Some(m) => m.clone(),
            None => {
                let m: Option<HashMap<String, String>> = self
                    .archive
                    .read_ref(table)
                    .and_then(|d| ui::string_table(&d))
                    .map(|v| v.into_iter().collect())
                    .ok();
                let m = Rc::new(m);
                self.tables.insert(table.to_string(), m.clone());
                m
            }
        };
        // `StringTable::GetText(name)`: a name the table lacks is an empty string
        // (`GetIndexFromName` −1); without a table (none loaded) the name is the text.
        match map.as_ref() {
            Some(m) => m.get(key).cloned().unwrap_or_default(),
            None => key.to_string(),
        }
    }
}

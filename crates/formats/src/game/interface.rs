//! Interface resources: the toolbar, the UI controller and status icons.

use super::*;

/// One entry of the in-game toolbar: a button or a sub-menu. The game builds each level of the
/// toolbar from the entries whose `parent` matches, in the order of `order`.
///
/// Original: `FUN_1400d74e0`; the toolbar is built by `FUN_1400bede0`.
#[derive(Debug, Clone)]
pub struct ToolbarItem {
    pub string_table: Option<String>,
    pub sprite_sheet: Option<String>,
    /// Tooltip string key.
    pub tooltip: String,
    /// Icon sprite name in `sprite_sheet`.
    pub sprite: String,
    // +0x48
    /// Action kind: 0 sub-menu, 1 build template, 6–10 time controls, …
    pub kind: u32,
    // +0x60
    /// Parent toolbar, `None` for the top level.
    pub parent: Option<String>,
    // +0x58
    /// Template placed by this entry, or the utility it opens.
    pub template: Option<String>,
    // +0x40
    /// Not identified yet.
    pub value: u32,
    // +0x45
    /// Listed in the toolbar.
    pub visible: bool,
    // +0x44
    /// Not identified yet.
    pub enabled: bool,
    // +0x4c
    /// Not identified yet.
    pub id: i32,
    // +0x50
    /// Sort key within its parent.
    pub order: i32,
}

/// Read a [`ToolbarItem`] from the bytes of its resource.
pub fn toolbar_item(data: &[u8]) -> Result<ToolbarItem> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("Toolbar") {
            return format_err(format!("expected Toolbar, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            Ok(ToolbarItem {
                string_table: r.external_ref()?,
                sprite_sheet: r.external_ref()?,
                tooltip: r.string()?,
                sprite: r.string()?,
                kind: r.u32()?,
                parent: r.external_ref()?,
                template: r.external_ref()?,
                value: r.u32()?,
                visible: r.bool()?,
                enabled: r.bool()?,
                id: r.i32()?,
                order: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

/// A template's toolbar component (`ToolbarDescription`): the icon, name and tooltip shown for
/// build and utility entries, and its placement messages.
///
/// Original: `FUN_140132f00`.
#[derive(Debug, Clone)]
pub struct ToolbarDescription {
    pub sprite_sheet: Option<String>,
    pub sprite: String,
    pub string_table: Option<String>,
    pub name: String,
    pub name_lower: String,
    pub tooltip: String,
    /// Message keys (`CreateOk`, `CreateBlocked`, …).
    pub messages: Vec<String>,
    // +0x64
    /// Not identified yet.
    pub category: i32,
    // +0x60
    /// Not identified yet.
    pub value: i32,
}

/// Read a [`ToolbarDescription`] from the bytes of its resource.
pub fn toolbar_description(data: &[u8]) -> Result<ToolbarDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ToolbarDescription") {
            return format_err(format!(
                "expected ToolbarDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(ToolbarDescription {
                sprite_sheet: r.external_ref()?,
                sprite: r.string()?,
                string_table: r.external_ref()?,
                name: r.string()?,
                name_lower: r.string()?,
                tooltip: r.string()?,
                messages: (0..r.count()?).map(|_| r.string()).collect::<Result<_>>()?,
                category: r.i32()?,
                value: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

/// One entry of a `UIDescription` controller table.
///
/// Original: `FUN_140133530`.
#[derive(Debug, Clone)]
pub struct UiController {
    /// CRC32 type id of the `UI::DialogController` to attach (0 = none, e.g. `StatusBarUI`
    /// `0x509d1aea`).
    pub type_id: u32,
    /// The element to instantiate into the named slot.
    pub element: Option<String>,
    // +0x10
    /// Controller configuration.
    pub config: Option<String>,
    // +0x18
    /// Tab label element, used when the slot is a `TabElement`.
    pub tab: Option<String>,
    /// Name of the slot element in the dialog (`userGroup0`, `userTitle0`).
    pub slot: String,
}

/// `UIDescription`: a utility window built from a host dialog with content placed into its named
/// slots, each with an optional controller.
///
/// Original: `FUN_140133350`.
#[derive(Debug, Clone)]
pub struct UiDescription {
    /// Host dialog, e.g. `Dialog\StandardDialog.rsc:resource`.
    pub dialog: Option<String>,
    // +0x40
    /// Not identified yet.
    pub flags: i32,
    pub controllers: Vec<UiController>,
    // +0x20
    /// Not identified yet.
    pub value: i32,
}

/// Read a [`UiDescription`] from the bytes of its resource.
pub fn ui_description(data: &[u8]) -> Result<UiDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("UIDescription") {
            return format_err(format!("expected UIDescription, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            let dialog = r.external_ref()?;
            let flags = r.i32()?;
            let mut controllers = Vec::new();
            for _ in 0..r.count()? {
                controllers.push(UiController {
                    type_id: r.u32()?,
                    element: r.external_ref()?,
                    config: r.external_ref()?,
                    tab: r.external_ref()?,
                    slot: r.string()?,
                });
            }
            Ok(UiDescription {
                dialog,
                flags,
                controllers,
                value: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x28; +0x30; +0x2c
/// `StatusIconDescription`: the icon sprite sheet, icon size and screen-constant flag. The height
/// offset is not serialized and stays 1.0.
///
/// Original: `FUN_140131650`, `FUN_140039280`.
#[derive(Debug, Clone)]
pub struct StatusIconDescription {
    pub sheet: Option<String>,
    pub size: f32,
    pub screen_constant: bool,
}

/// Read a [`StatusIconDescription`] from the bytes of its resource.
pub fn status_icon_description(data: &[u8]) -> Result<StatusIconDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("StatusIconDescription") {
            return format_err(format!(
                "expected StatusIconDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(StatusIconDescription {
                sheet: r.external_ref()?,
                size: r.f32()?,
                screen_constant: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

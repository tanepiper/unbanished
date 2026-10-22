//! Dialog resources: a dialog's root element and the bindings of its named elements.

use super::*;

/// A controller attached to a dialog's element.
#[derive(Debug, Clone)]
pub struct DialogBinding {
    /// Application controller type ID (CRC32 of its class name).
    pub unknown: u32,
    /// Optional element description created beneath the named host.
    pub source: Ref,
    /// Optional controller configuration resource.
    pub target: Ref,
    /// Host element path identifier, or empty for the dialog root.
    pub name: String,
}

/// Original: `UI::Dialog`, Runtime `0x180115950`.
#[derive(Debug, Clone)]
pub struct Dialog {
    /// Not identified yet.
    pub unknown_40: u32,
    /// The root element.
    pub root: Ref,
    /// The sprite sheet of the background drawn behind the dialog.
    pub background_sheet: Ref,
    /// The background's sprite name.
    pub background_sprite: String,
    /// Not identified yet.
    pub unknown_30: u32,
    /// Elements replaced by name: the replacement resource and the name of the element it replaces.
    pub named: Vec<(Ref, String)>,
    /// The dialog's elements.
    pub children: Vec<Ref>,
    /// The controllers attached to elements.
    pub bindings: Vec<DialogBinding>,
    /// Associated resources, including nested dialogs and string tables.
    pub dialogs: Vec<Ref>,
    /// Not identified yet.
    pub unknown_34: i32,
}

/// Read a [`Dialog`] from the bytes of its resource.
pub fn dialog(data: &[u8]) -> Result<Dialog> {
    expect(data, "Dialog", |r| {
        r.block(|r, _| {
            let unknown_40 = r.u32()?;
            let root = r.external_ref()?;
            let background_sheet = r.external_ref()?;
            let background_sprite = r.string()?;
            let unknown_30 = r.u32()?;
            let named = (0..r.count()?)
                .map(|_| Ok((r.external_ref()?, r.string()?)))
                .collect::<Result<_>>()?;
            let children = refs(r)?;
            let bindings = (0..r.count()?)
                .map(|_| {
                    Ok(DialogBinding {
                        unknown: r.u32()?,
                        source: r.external_ref()?,
                        target: r.external_ref()?,
                        name: r.string()?,
                    })
                })
                .collect::<Result<_>>()?;
            let dialogs = refs(r)?;
            Ok(Dialog {
                unknown_40,
                root,
                background_sheet,
                background_sprite,
                unknown_30,
                named,
                children,
                bindings,
                dialogs,
                unknown_34: r.i32()?,
            })
        })
    })
}

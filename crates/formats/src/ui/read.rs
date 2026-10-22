//! The readers of each element class's resource body.

use super::*;

pub(super) fn read_button(r: &mut Reader) -> Result<Button> {
    r.block(|r, _| {
        let mut states: [Ref; 4] = Default::default();
        for s in &mut states {
            *s = r.external_ref()?;
        }
        let content = r.external_ref()?;
        let colors = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
        Ok(Button {
            states,
            content,
            colors,
        })
    })
}

pub(super) fn read_text(r: &mut Reader) -> Result<Text> {
    r.block(|r, _| {
        Ok(Text {
            font: r.external_ref()?,
            string_table: r.external_ref()?,
            text: r.string()?,
            shadow: r.bool()?,
            wrap: r.bool()?,
            max_width: r.i32()?,
            justify: r.u32()?,
            ellipsis_width: r.i32()?,
        })
    })
}

pub(super) fn read_group(r: &mut Reader) -> Result<Vec<Ref>> {
    r.block(|r, _| refs(r))
}

pub(super) fn read_image(r: &mut Reader) -> Result<(Ref, String, [i32; 2])> {
    r.block(|r, _| Ok((r.external_ref()?, r.string()?, i32s(r)?)))
}

/// Decode any `UI::*Description` element given its class name.
pub(super) fn read_element(class: &'static str, r: &mut Reader) -> Result<Element> {
    let base = ElementBase::read(r)?;
    let kind = match class {
        "GroupDescription" => ElementKind::Group {
            children: read_group(r)?,
        },
        "PageDescription" => {
            let children = read_group(r)?;
            ElementKind::Page {
                children,
                page: r.block(|r, _| r.i32())?,
            }
        }
        "RadioDescription" => {
            let children = read_group(r)?;
            ElementKind::Radio {
                children,
                selected: r.block(|r, _| r.i32())?,
            }
        }
        "RibbonDescription" => r.block(|r, _| {
            Ok(ElementKind::Ribbon {
                vertical: r.bool()?,
                uniform: r.bool()?,
                spacing: r.i32()?,
                children: refs(r)?,
            })
        })?,
        "BorderDescription" => r.block(|r, _| {
            Ok(ElementKind::Border {
                sprite_sheet: r.external_ref()?,
                sprites: strings(r)?,
            })
        })?,
        "ImageDescription" => {
            let (sprite_sheet, sprite, size) = read_image(r)?;
            ElementKind::Image {
                sprite_sheet,
                sprite,
                size,
            }
        }
        "FrustumOutlineDescription" => {
            let (sprite_sheet, sprite, size) = read_image(r)?;
            ElementKind::FrustumOutline {
                sprite_sheet,
                sprite,
                size,
            }
        }
        "AnimatedImageDescription" => {
            let (sprite_sheet, sprite, size) = read_image(r)?;
            r.block(|r, _| {
                let frames = (0..r.count()?).map(|_| r.string()).collect::<Result<_>>()?;
                Ok(ElementKind::AnimatedImage {
                    sprite_sheet,
                    sprite,
                    size,
                    frames,
                    rate: r.i32()?,
                })
            })?
        }
        "ButtonDescription" => ElementKind::Button(read_button(r)?),
        "CheckDescription" => {
            let button = read_button(r)?;
            ElementKind::Check {
                button,
                check: r.block(|r, _| r.external_ref())?,
            }
        }
        "ComboDescription" => {
            let button = read_button(r)?;
            r.block(|r, _| {
                Ok(ElementKind::Combo {
                    button,
                    refs: [r.external_ref()?, r.external_ref()?, r.external_ref()?],
                })
            })?
        }
        "TextDescription" => ElementKind::Text(read_text(r)?),
        "LabelDescription" => {
            let text = read_text(r)?;
            r.block(|r, _| {
                let sprite_sheet = r.external_ref()?;
                let sprite = r.string()?;
                let text_alignment = r.u32()?;
                let orientation = r.u32()?;
                let [min_text_width, gap, w, h] = i32s(r)?;
                Ok(ElementKind::Label(Label {
                    text,
                    sprite_sheet,
                    sprite,
                    text_alignment,
                    orientation,
                    min_text_width,
                    gap,
                    icon_size: [w, h],
                }))
            })?
        }
        "ClipRegionDescription" => r.block(|r, _| {
            Ok(ElementKind::ClipRegion {
                unknown: i32s(r)?,
                content: r.external_ref()?,
            })
        })?,
        "ContainerDescription" => r.block(|r, _| {
            Ok(ElementKind::Container {
                unknown: i32s(r)?,
                refs: [r.external_ref()?, r.external_ref()?, r.external_ref()?],
                unknown_bools: [r.bool()?, r.bool()?],
            })
        })?,
        "EditDescription" => r.block(|r, _| {
            Ok(ElementKind::Edit {
                font: r.external_ref()?,
                text: r.string()?,
                max_length: r.i32()?,
                unknown_bool: r.bool()?,
                unknown_string: r.string()?,
            })
        })?,
        "GraphDescription" => r.block(|r, _| {
            Ok(ElementKind::Graph {
                unknown: i32s(r)?,
                unknown_u32: r.u32()?,
                sprite_sheet: r.external_ref()?,
                sprite: r.string()?,
            })
        })?,
        "LayoutDescription" => r.block(|r, _| {
            let items = (0..r.count()?)
                .map(|_| {
                    Ok(LayoutItem {
                        element: r.external_ref()?,
                        strings: strings(r)?,
                        unknown_u32: [r.u32()?, r.u32()?],
                        unknown_i32: i32s(r)?,
                        flag: r.bool()?,
                    })
                })
                .collect::<Result<_>>()?;
            Ok(ElementKind::Layout { items })
        })?,
        "ListDescription" => r.block(|r, _| {
            Ok(ElementKind::List {
                refs: [r.external_ref()?, r.external_ref()?],
                sprite_sheet: r.external_ref()?,
                sprite: r.string()?,
                unknown: [r.u32()?, r.u32()?],
            })
        })?,
        "NumberEditDescription" => r.block(|r, _| {
            Ok(ElementKind::NumberEdit {
                font: r.external_ref()?,
                unknown: i32s(r)?,
                unknown_bool: r.bool()?,
            })
        })?,
        "NumberDescription" => r.block(|r, _| {
            Ok(ElementKind::Number {
                font: r.external_ref()?,
                color: r.u32()?,
                unknown_int: r.i32()?,
                unknown_float: r.f32()?,
                unknown_bool: r.bool()?,
            })
        })?,
        "SliderDescription" => r.block(|r, _| {
            Ok(ElementKind::Slider {
                track: r.external_ref()?,
                sprite_sheet: r.external_ref()?,
                sprites: strings(r)?,
                unknown: i32s(r)?,
                range: [r.f32()?, r.f32()?],
                unknown_b0: r.i32()?,
            })
        })?,
        "SpinnerDescription" => r.block(|r, _| {
            Ok(ElementKind::Spinner {
                refs: [r.external_ref()?, r.external_ref()?, r.external_ref()?],
                unknown: r.i32()?,
            })
        })?,
        "TabDescription" => r.block(|r, _| {
            Ok(ElementKind::Tab {
                refs: [r.external_ref()?, r.external_ref()?, r.external_ref()?],
                sprite_sheet: r.external_ref()?,
                sprites: strings(r)?,
                unknown: r.i32()?,
                tabs: (0..r.count()?)
                    .map(|_| Ok((r.external_ref()?, r.external_ref()?)))
                    .collect::<Result<_>>()?,
            })
        })?,
        "ProgressDescription" => r.block(|r, _| {
            Ok(ElementKind::Progress {
                color: r.u32()?,
                sprite_sheet: r.external_ref()?,
                sprites: strings(r)?,
                unknown_ref: r.external_ref()?,
            })
        })?,
        "ScrollDescription" => r.block(|r, _| {
            Ok(ElementKind::Scroll {
                sprite_sheet: r.external_ref()?,
                sprites: strings(r)?,
                vertical: r.bool()?,
                unknown: r.i32()?,
                unknown_u32: [r.u32()?, r.u32()?],
            })
        })?,
        "TableDescription" => r.block(|r, _| {
            let unknown = i32s(r)?;
            let cells = (0..r.count()?)
                .map(|_| {
                    Ok(TableCell {
                        column: r.i32()?,
                        row: r.i32()?,
                        element: r.external_ref()?,
                    })
                })
                .collect::<Result<_>>()?;
            Ok(ElementKind::Table { unknown, cells })
        })?,
        "DocumentTextDescription" => r.block(|r, _| {
            Ok(ElementKind::DocumentText(DocumentText {
                string_table: r.external_ref()?,
                text: r.string()?,
                sprite_sheet: r.external_ref()?,
                bullets: (0..r.count()?).map(|_| r.string()).collect::<Result<_>>()?,
                underline_sprite: r.string()?,
                link_colors: [r.u32()?, r.u32()?, r.u32()?],
                indent: r.i32()?,
                width: r.i32()?,
                fonts: refs(r)?,
                sheets: refs(r)?,
                colors: (0..r.count()?).map(|_| r.u32()).collect::<Result<_>>()?,
                images: r.external_ref()?,
            }))
        })?,
        _ => return Err(Error::Unsupported(format!("element class {class}"))),
    };
    Ok(Element { class, base, kind })
}

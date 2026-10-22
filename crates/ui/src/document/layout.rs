//! Laying out a document: its items in rows, wrapped to the element's width.

use super::*;

impl Ui {
    pub(crate) fn layout_document(&self, doc: &mut Document) {
        doc.items.clear();
        doc.links.clear();
        let mut row = Row {
            start: 0,
            x: 0,
            y: 0,
            left: 0,
            right: doc.width.max(1),
            align: 'l',
            vertical: 'b',
            font: 0,
            color: 0,
            link: None,
            restore_left: 0,
        };
        let mut columns = [0, 0];
        let tokens = tokenize(&doc.text);
        for (token_index, token) in tokens.iter().cloned().enumerate() {
            match token {
                Token::Word(word) => {
                    let font = doc.fonts.get(row.font).copied().flatten();
                    let text = format!("{word} ");
                    let layout = font
                        .map(|f| TextLayout::compute(&self.res, f, &text, None, 0))
                        .unwrap_or_default();
                    let size = [layout.width, layout.height];
                    let data = TextData {
                        font,
                        text,
                        max_width: 0,
                        justify: 0,
                        wrap: false,
                        shadow: false,
                        ellipsis_width: 0,
                        layout,
                    };
                    self.document_item(doc, &mut row, Content::Text(data), size);
                }
                Token::Command(command, suffix, arg) => {
                    match (command, suffix) {
                        ('n', _) => self.document_end_row(doc, &mut row),
                        ('p', _) => {
                            self.document_end_row(doc, &mut row);
                            self.document_end_row(doc, &mut row);
                        }
                        ('f', Some(d)) => {
                            if let Some(index) = d.to_digit(10) {
                                if (index as usize) < doc.fonts.len() {
                                    row.font = index as usize;
                                }
                            }
                        }
                        ('c', Some(d)) => {
                            if let Some(index) = d.to_digit(10) {
                                if (index as usize) < doc.colors.len() {
                                    row.color = index as usize;
                                }
                            }
                        }
                        ('j', Some(j @ ('l' | 'r' | 'c'))) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            row.align = j;
                        }
                        ('v', Some(v @ ('t' | 'c' | 'b'))) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            row.vertical = v;
                        }
                        ('m', Some('l' | 'r')) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            let value = arg.parse::<i32>().unwrap_or(0);
                            if suffix == Some('l') {
                                row.left = value.max(0);
                                row.x = row.left;
                            } else {
                                row.right = if value < 0 { doc.width } else { value };
                            }
                        }
                        ('m', Some('b' | 'c' | 'e')) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            match suffix {
                                Some('b') => columns = [row.y, row.y],
                                Some('c') => {
                                    columns[1] = columns[1].max(row.y);
                                    row.y = columns[0];
                                }
                                _ => {
                                    columns[1] = columns[1].max(row.y);
                                    row.y = columns[1];
                                }
                            }
                            row.x = row.left;
                        }
                        ('l', Some('b' | 'e')) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            row.left = if suffix == Some('b') {
                                row.left + doc.indent
                            } else {
                                row.left - doc.indent
                            }
                            .max(0);
                            row.x = row.left;
                        }
                        ('l', Some('i')) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            if let Some(sprite) = doc.bullets.get(row.font).copied().flatten() {
                                row.left -= sprite.width;
                                row.x = row.left;
                                row.restore_left = sprite.width;
                                self.document_item(
                                    doc,
                                    &mut row,
                                    Content::Sprite(sprite),
                                    [sprite.width, sprite.height],
                                );
                            }
                        }
                        ('l', Some('o')) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            // GetNextItem previews the next item's width for a hanging list marker.
                            let font = doc.fonts.get(row.font).copied().flatten();
                            let marker_width = tokens[token_index + 1..]
                                .iter()
                                .find_map(|token| match token {
                                    Token::Word(word) => font.map(|f| {
                                        TextLayout::compute(
                                            &self.res,
                                            f,
                                            &format!("{word} "),
                                            None,
                                            0,
                                        )
                                        .width
                                    }),
                                    _ => None,
                                })
                                .unwrap_or(doc.indent);
                            row.y += font.map_or(0, |f| self.res.fonts[f].font.size / 4);
                            row.left -= marker_width;
                            row.x = row.left;
                            row.restore_left = marker_width;
                        }
                        ('a', Some('b')) => row.link = (!arg.is_empty()).then_some(arg),
                        ('a', Some('e')) => row.link = None,
                        ('i', Some(d)) => {
                            if let Some(sheet) = d
                                .to_digit(10)
                                .and_then(|i| doc.sheets.get(i as usize))
                                .copied()
                                .flatten()
                            {
                                if let Some(s) = self.res.sheets[sheet].sheet.sprite(&arg) {
                                    let sprite = SpriteRef {
                                        sheet,
                                        width: s.width,
                                        height: s.height,
                                        uv: s.uv,
                                    };
                                    self.document_item(
                                        doc,
                                        &mut row,
                                        Content::Sprite(sprite),
                                        [s.width, s.height],
                                    );
                                }
                            }
                        }
                        ('t', _) => {
                            let width = doc.indent;
                            self.document_item(doc, &mut row, Content::Space, [width, 0]);
                        }
                        ('s', _) => {
                            let width = doc.fonts.get(row.font).copied().flatten().map_or(0, |f| {
                                TextLayout::compute(&self.res, f, " ", None, 0).width
                            });
                            self.document_item(doc, &mut row, Content::Space, [width, 0]);
                        }
                        ('d', _) => {
                            if let Some(capture) = doc.captures.get(&capture_name(&arg)).cloned() {
                                let size = capture.size;
                                self.document_item(doc, &mut row, Content::Capture(capture), size);
                            } else {
                                log::warn!("document capture {arg:?} was not found in its image collection");
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if row.start < doc.items.len() {
            self.document_end_row(doc, &mut row);
        }
        doc.size = [doc.width.max(1), row.y];
        for item in &doc.items {
            if let Some(target) = &item.link {
                doc.links.push((
                    target.clone(),
                    [
                        item.pos[0],
                        item.pos[1],
                        item.pos[0] + item.size[0],
                        item.pos[1] + item.size[1],
                    ],
                ));
            }
        }
    }

    pub(super) fn document_item(
        &self,
        doc: &mut Document,
        row: &mut Row,
        content: Content,
        size: [i32; 2],
    ) {
        if row.start < doc.items.len() && row.x + size[0] > row.right {
            self.document_end_row(doc, row);
        }
        let color = if row.link.is_some() {
            doc.link_colors[0]
        } else {
            doc.colors.get(row.color).copied().unwrap_or(0xFFFF_FFFF)
        };
        doc.items.push(Item {
            content,
            pos: [row.x, row.y],
            size,
            color,
            link: row.link.clone(),
        });
        row.x += size[0];
    }

    pub(super) fn document_end_row(&self, doc: &mut Document, row: &mut Row) {
        let default_height = doc
            .fonts
            .get(row.font)
            .copied()
            .flatten()
            .map_or(0, |f| self.res.fonts[f].font.size);
        let height = doc.items[row.start..]
            .iter()
            .map(|i| i.size[1])
            .max()
            .unwrap_or(default_height);
        let width = row.x - row.left;
        let shift = match row.align {
            'r' => row.right - row.left - width,
            'c' => (row.right - row.left - width) / 2,
            _ => 0,
        };
        for item in &mut doc.items[row.start..] {
            item.pos[0] += shift;
            item.pos[1] = row.y
                + match row.vertical {
                    't' => 0,
                    'c' => (height - item.size[1]) / 2,
                    _ => height - item.size[1],
                };
        }
        row.y += height;
        row.start = doc.items.len();
        row.left += row.restore_left;
        row.restore_left = 0;
        row.x = row.left;
    }
}

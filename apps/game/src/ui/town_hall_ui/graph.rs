//! The graph page: the statistics over time.

use super::*;

/// `FUN_1400963f0`: a series' sum over samples `from..=to`, clamped to what exists.
pub(super) fn sum(s: &Statistics, series: usize, from: i32, to: i32) -> i32 {
    let Some(v) = s.series.get(series) else {
        return 0;
    };
    let from = from.max(0);
    let to = to.min(v.len() as i32 - 1);
    if to < from {
        return 0;
    }
    v[from as usize..=to as usize].iter().sum()
}

impl TownHallUi {
    /// `FUN_140083a80`: the chosen series over the chosen time range, the y axis rounded up.
    pub(super) fn graph(&mut self, ui: &mut Ui, statistics: &Statistics, force: bool) {
        let Some(g) = self.graph else { return };
        let time = combo_index(ui, self.time).min(6);
        let kind = combo_index(ui, self.series).min(22);
        let samples = statistics.series.first().map_or(0, |s| s.len());
        if !force && self.drawn == Some((samples, time, kind)) {
            return;
        }
        self.drawn = Some((samples, time, kind));
        // Each range's sample step, point count and x divisions (`SetXDivisions`, `+0xe4`).
        let (step, points, divisions) = [
            (1, 48, 12),
            (1, 96, 24),
            (2, 120, 5),
            (4, 120, 10),
            (8, 150, 25),
            (16, 150, 50),
            (32, 150, 100),
        ][time];
        // `AddGraph`'s colour for each of the (up to four) series, which its key's sprite
        // takes too (`LabelElement::SetSpriteColor`).
        const COLORS: [u32; 4] = [0xFFFF_FF00, 0xFF00_FF00, 0xFF80_80FF, 0xFFFF_0000];
        const SLOTS: [usize; 21] = [
            5, 0, 1, 2, 3, 9, 4, 6, 7, 8, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        ];
        let series: Vec<usize> = match kind {
            0 => vec![0, 1, 2, 3],
            1 => vec![4, 6, 5, 7],
            k => vec![8 + 3 * SLOTS[(k - 2).min(20)]],
        };
        let points = points + 1;
        let mut top = 0;
        let mut lines = Vec::new();
        for &s in &series {
            let data = statistics.series.get(s).map_or(&[][..], |v| &v[..]);
            let have = data.len() as i32;
            let mut line = Vec::new();
            let mut from = have - points * step;
            if have / step < points {
                line.extend(std::iter::repeat_n(0, (points - have / step) as usize));
                from = 0;
            }
            let mut i = from.max(0);
            while i < have {
                let v = data[i as usize];
                top = top.max(v);
                line.push(v);
                i += step;
            }
            lines.push((COLORS[lines.len()], line));
        }
        let top = top.max(5);
        let unit = if top < 10 {
            1
        } else if top < 100 {
            10
        } else if top < 1000 {
            100
        } else {
            1000
        };
        let top = (top - 1 + unit) / unit * unit;
        ui.set_graph(g, top as f32, divisions, lines);
        for (i, label) in self.y_labels.iter().enumerate() {
            let v = top as f32 * (4 - i) as f32 * 0.25;
            set_number(ui, *label, v as i32);
        }
        for (i, legend) in self.legends.iter().enumerate() {
            let Some(l) = *legend else { continue };
            match series.get(i) {
                Some(&s) => {
                    let text = ui
                        .res
                        .string_at("Dialog\\StringTable.rsc:graphKeys", s as i32);
                    ui.set_text(l, &text);
                    ui.set_label_sprite_color(l, COLORS[i]);
                    ui.set_visible(l, true);
                }
                None => ui.set_visible(l, false),
            }
        }
        let _ = self.x_min;
    }
}

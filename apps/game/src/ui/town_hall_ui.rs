//! `TownHallUI` (type `0xcd3b5953`, vtable slots: bind `FUN_1400864d0`, refresh
//! `FUN_140082a20`, per-frame `FUN_140083a20`, events `FUN_140081650`): the town hall's tabs
//! (`Dialog\TownHall.rsc:tabs`). See docs/BUILDING-WINDOWS.md.
//!
//! - Overview (`FUN_140085cc0` / `FUN_140082500`): the year and month, homes, families,
//!   citizens, adults, students, children, the shares clothed and educated, and the average
//!   health and happiness as hearts and stars.
//! - Professions (`FUN_140085630` / `FUN_1400833c0`): per profession its workers (a spinner,
//!   `0x1966 + i`), jobs, workplaces and a button cycling through them (`0x1999 + i`).
//! - Production (`FUN_140085100` / `FUN_140082180`): per stock slot the count, this year's
//!   use and production and the previous period's (`productionCombo`: 1 to 100 years).
//! - Inventory (`FUN_140084b30` / `FUN_140081cc0`, `FUN_140082a60`): every raw material held in
//!   storage, trading posts or homes (`inventoryCombo`), by name or by quantity (`sortCombo`).
//! - Graphs (`FUN_140084570` / `FUN_140083a80`): up to four series of the statistics over a
//!   time range (`timeCombo`, id `0x1964`; `graphCombo`, `0x1965`).
//! - Farming (`FUN_1400840c0` / `FUN_1400816f0`): the known crops, trees and livestock.

mod bind;
mod graph;
mod inventory;
mod pages;
use graph::sum;
use inventory::select_description;

use unbanished_formats::pkg::Archive;
use unbanished_game::work::tracker::Statistics;
use unbanished_ui::{Control, Kind, NodeId, Ui};

use crate::start::StartingTown;
use crate::ui::building_ui::WorkPlaceClick;

/// What the town hall reads besides the town: the stock manager's slots and history.
pub struct TownContext<'a> {
    pub stock_counts: &'a [i32; 21],
    pub statistics: &'a Statistics,
}

/// One sample every 75 s: 48 a year.
const SAMPLES_PER_YEAR: i32 = 48;

fn text_of(ui: &Ui, id: NodeId) -> String {
    match &ui.node(id).kind {
        Kind::Text(t) => t.text.clone(),
        Kind::Label { text, .. } => text.text.clone(),
        Kind::Control(Control::Edit(e)) => e.text.text.clone(),
        _ => String::new(),
    }
}

fn set_number(ui: &mut Ui, n: Option<NodeId>, v: i32) {
    if let Some(n) = n {
        ui.set_text(n, &v.to_string());
    }
}

pub(crate) fn combo_index(ui: &Ui, combo: Option<NodeId>) -> usize {
    let Some(c) = combo else { return 0 };
    let list = match &ui.node(c).kind {
        Kind::Control(Control::Combo { list, .. }) => *list,
        _ => return 0,
    };
    match &ui.node(list).kind {
        Kind::List(l) => l.selected.unwrap_or(0),
        _ => 0,
    }
}

/// A table's cells, the first re-instantiated `count` times (`TableElement::SetSize` and a
/// clone of cell 0 per cell).
pub(crate) fn fill_table(ui: &mut Ui, table: NodeId, count: usize) -> Vec<NodeId> {
    let template = match &ui.node(table).kind {
        Kind::Control(Control::Table { cells, .. }) => cells.first().map(|c| c.2),
        _ => None,
    };
    let Some(reference) = template.map(|t| ui.node(t).reference.clone()) else {
        return Vec::new();
    };
    if let Kind::Control(Control::Table { cells, rows, .. }) = &mut ui.node_mut(table).kind {
        cells.clear();
        rows.clear();
    }
    let mut out = Vec::new();
    for i in 0..count {
        if let Some(cell) = ui.instantiate(&reference) {
            ui.set_table_child(table, i, cell);
            out.push(cell);
        }
    }
    out
}

struct ProfessionRow {
    spinner: Option<NodeId>,
    jobs: Option<NodeId>,
    locations: Option<NodeId>,
    goto: Option<NodeId>,
}

pub struct TownHallUi {
    /// `+0x30`: the tabs.
    tabs: Option<NodeId>,
    year: Option<NodeId>,
    month: Option<NodeId>,
    homes: Option<NodeId>,
    families: Option<NodeId>,
    citizens: Option<NodeId>,
    adults: Option<NodeId>,
    students: Option<NodeId>,
    children: Option<NodeId>,
    clothed: Option<(NodeId, String)>,
    educated: Option<(NodeId, String)>,
    hearts: Option<NodeId>,
    stars: Option<NodeId>,
    professions: Vec<ProfessionRow>,
    /// Per profession the workplace the goto button last showed.
    cursors: Vec<usize>,
    period: Option<NodeId>,
    prev_headers: [Option<(NodeId, String)>; 2],
    /// Per stock slot: count, used, made, previous used, previous made.
    limits: Vec<[Option<NodeId>; 5]>,
    inventory: Vec<(Option<NodeId>, Option<NodeId>)>,
    inventory_type: Option<NodeId>,
    sort: Option<NodeId>,
    graph: Option<NodeId>,
    time: Option<NodeId>,
    series: Option<NodeId>,
    x_min: Option<NodeId>,
    y_labels: [Option<NodeId>; 5],
    legends: [Option<NodeId>; 4],
    /// `+0x148`: the samples the graph was drawn with.
    drawn: Option<(usize, usize, usize)>,
    agri: Vec<NodeId>,
    filled: bool,
}

impl TownHallUi {
    pub fn refresh(
        &mut self,
        ui: &mut Ui,
        archive: &Archive,
        town: &StartingTown,
        ctx: &TownContext,
    ) {
        self.overview(ui, town);
        self.production(ui, town, ctx);
        self.inventory(ui, archive, town);
        if !self.filled {
            self.filled = true;
            self.agriculture(ui, archive, town);
        }
        // The per-frame slot (`FUN_140083a20`): the graph when a sample came in, the
        // professions.
        self.graph(ui, ctx.statistics, false);
        self.professions(ui, town);
    }

    /// `FUN_140081650`: a spinner (`0x15`, `0x1966 + i`), a goto button (`0x12`,
    /// `0x1999 + i`) or a graph combo (`0x14`).
    pub fn click(&mut self, ui: &Ui, town: &StartingTown, node: NodeId) -> Option<WorkPlaceClick> {
        for (i, row) in self.professions.iter().enumerate() {
            if row
                .spinner
                .is_some_and(|s| s == node || ui.descendant(s, node))
            {
                let (counts, _) = town.profession_counts();
                let v = spinner_value(ui, row.spinner?)?;
                let free = counts.first().copied().unwrap_or(0);
                let count = counts.get(i).copied().unwrap_or(0);
                if i == 0 {
                    return None;
                }
                return Some(WorkPlaceClick::Command(
                    unbanished_game::work::professions::Command::SetCount {
                        profession: i,
                        count: v.clamp(0, count + free),
                    },
                ));
            }
            if row
                .goto
                .is_some_and(|b| b == node || ui.descendant(b, node))
            {
                // `FUN_140083170`: the next of the profession's workplaces.
                let list = town.construction.workforce.registry.get(&i)?;
                if list.is_empty() {
                    return None;
                }
                let k = self.cursors[i] % list.len();
                self.cursors[i] = k + 1;
                return Some(WorkPlaceClick::ShowBuilding(list[k]));
            }
        }
        None
    }

    /// A combo choice: the graph is drawn again.
    pub fn select(&mut self, ui: &mut Ui, statistics: &Statistics, node: NodeId) -> bool {
        if Some(node) == self.time || Some(node) == self.series {
            self.graph(ui, statistics, true);
            return true;
        }
        Some(node) == self.period || Some(node) == self.inventory_type || Some(node) == self.sort
    }
}

fn set_spinner(ui: &mut Ui, id: NodeId, value: i32) {
    let number = match &mut ui.node_mut(id).kind {
        Kind::Control(Control::Spinner {
            parts, value: v, ..
        }) => {
            *v = value;
            parts[0]
        }
        _ => None,
    };
    if let Some(n) = number {
        ui.set_text(n, &value.to_string());
    }
}

fn spinner_value(ui: &Ui, id: NodeId) -> Option<i32> {
    match &ui.node(id).kind {
        Kind::Control(Control::Spinner { value, .. }) => Some(*value),
        _ => None,
    }
}

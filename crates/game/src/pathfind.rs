//! Map pathfinding (see docs/CITIZEN-AI.md):
//!
//! - connectivity regions `+0x26` (`FUN_1400a1950` / `FUN_1400a1640`);
//! - the path manager at map `+0x68` (`FUN_1400abcc0`), whose worker thread executes queued
//!   commands (`FUN_1400ab900`): 0 walk mask, 1 per-type cost, 4 search options, 5 find path;
//! - the A* search (`FUN_14001d760` / `FUN_14001d9a0`);
//! - request validation (`FUN_1400a2480`) and the per-mover cost tables (`FUN_1400a3a20`);
//! - the nearest-allowed-neighbour helper (`FUN_1400a21c0`).
//!
//! The original solves on a thread and movers poll a result slot. The port solves when the
//! request is made; [`MoveTo`](crate::actions) still reads the result on its next update.

use std::cmp::Reverse;
use std::collections::{BTreeSet, VecDeque};

use crate::terrain::CellType;

/// Cell types a walker may enter (0–4, 9–11).
pub const WALKABLE: u32 = 0xe1f;
/// Water and blocking types (5–8): requests touching them skip the region check.
pub const WATER: u32 = 0x1e0;
/// `FUN_14001d760`: longest path a search returns.
pub const MAX_PATH: usize = 0x4000;

pub fn bit(t: CellType) -> u32 {
    1u32 << (t.0 & 0x1f)
}

/// `FUN_1400a1950`: label connected walkable cells in scan order, starting at 1 (0 = none).
pub fn regions(size: usize, types: &[CellType]) -> Vec<u16> {
    let mut region = vec![0u16; types.len()];
    let mut id: u16 = 1;
    for i in 0..types.len() {
        if bit(types[i]) & WALKABLE != 0 && region[i] == 0 {
            fill(size, types, &mut region, i, id);
            id = id.wrapping_add(1);
        }
    }
    region
}

/// `FUN_1400a1640`: FIFO scanline fill. A run is widened by one cell for the rows above and
/// below only when that cell is strictly inside the row (`row_start < left - 1`,
/// `right + 1 < row_end`), so runs touch diagonal neighbours except at the map edge.
fn fill(size: usize, types: &[CellType], region: &mut [u16], start: usize, id: u16) {
    let n = size as i32;
    let walk = |c: i32| bit(types[c as usize]) & WALKABLE != 0;
    let mut queue = VecDeque::from([start as i32]);
    while let Some(c) = queue.pop_front() {
        let row = c / n;
        let (row_start, row_end) = (n * row, (row + 1) * n - 1);
        if region[c as usize] != 0 || !walk(c) {
            continue;
        }
        let mut left = c;
        while left - 1 >= row_start && walk(left - 1) {
            left -= 1;
        }
        let mut right = c;
        while right + 1 <= row_end && walk(right + 1) {
            right += 1;
        }
        for k in left..=right {
            region[k as usize] = id;
        }
        if row_start < left - 1 {
            left -= 1;
        }
        if right + 1 < row_end {
            right += 1;
        }
        for (ok, step) in [(row > 0, -n), (row < n - 1, n)] {
            if !ok {
                continue;
            }
            for u in left..=right {
                let k = u + step;
                if region[k as usize] == 0 && walk(k) {
                    queue.push_back(k);
                }
            }
        }
    }
}

/// Per-type cost tables a mover selects (`FUN_1400a3a20`, MoveTo `+0x470`) and whether the
/// search may step diagonally.
pub fn mode_costs(mode: u32) -> Option<(&'static [(u8, i32)], bool)> {
    const CITIZEN: [(u8, i32); 11] = [
        (0, 3),
        (1, 3),
        (2, 4),
        (3, 8),
        (4, 16),
        (5, 16),
        (6, 16),
        (7, 16),
        (8, 16),
        (9, 3),
        (10, 4),
    ];
    const MODE1: [(u8, i32); 11] = [
        (0, 16),
        (1, 16),
        (2, 4),
        (3, 4),
        (4, 16),
        (5, 16),
        (6, 16),
        (7, 4),
        (8, 4),
        (9, 4),
        (10, 4),
    ];
    const UNIFORM: [(u8, i32); 11] = [
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (9, 1),
        (10, 1),
    ];
    const MODE3: [(u8, i32); 11] = [
        (0, 10),
        (1, 10),
        (2, 10),
        (3, 10),
        (4, 10),
        (5, 10),
        (6, 10),
        (7, 5),
        (8, 1),
        (9, 5),
        (10, 10),
    ];
    match mode {
        0 => Some((&CITIZEN, true)),
        1 => Some((&MODE1, true)),
        2 => Some((&UNIFORM, false)),
        3 => Some((&MODE3, true)),
        _ => None,
    }
}

#[derive(Clone, Copy, Default)]
struct Node {
    parent: i32,
    /// `+0x0c` / `+0x10`: equal to the search id while open / closed.
    open: i32,
    closed: i32,
    g: i32,
    /// Open-set key while open: `(f, counter)`.
    key: (i32, i32),
}

/// The path manager's search state (`FUN_1400abcc0` and the grid at `+0x78`).
pub struct Pathfinder {
    size: i32,
    /// Path grid bytes: cell type `& 0x7f`, `0x80` blocked (commands 2 and 3).
    cells: Vec<u8>,
    /// `+0x50`: cost per type (command 1).
    costs: [i32; 32],
    /// `+0xd0`: allowed types (command 0).
    pub mask: u32,
    /// `+0xd4..+0xd6` (command 4): diagonal steps, Manhattan heuristic, honour blocked bit.
    diagonal: bool,
    manhattan: bool,
    check_blocked: bool,
    search: i32,
    counter: i32,
    nodes: Vec<Node>,
    open: BTreeSet<((i32, Reverse<i32>), i32)>,
}

impl Pathfinder {
    /// `FUN_1400a3d90`: built from the map with diagonal steps on.
    pub fn new(size: usize, types: &[CellType]) -> Pathfinder {
        Pathfinder {
            size: size as i32,
            cells: types.iter().map(|t| t.0 & 0x7f).collect(),
            costs: [1; 32],
            mask: 0,
            diagonal: true,
            manhattan: false,
            check_blocked: false,
            search: 0,
            counter: 0,
            nodes: vec![Node::default(); types.len()],
            open: BTreeSet::new(),
        }
    }

    /// Command 2: a cell's type changed.
    pub fn set_type(&mut self, cell: usize, t: CellType) {
        self.cells[cell] = self.cells[cell] & 0x80 | t.0 & 0x7f;
    }

    /// Command 3.
    pub fn set_blocked(&mut self, cell: usize, blocked: bool) {
        self.cells[cell] = self.cells[cell] & 0x7f | if blocked { 0x80 } else { 0 };
    }

    /// `FUN_1400a3a20`: select a mover's cost table and diagonal option.
    pub fn set_mode(&mut self, mode: u32) {
        if let Some((costs, diagonal)) = mode_costs(mode) {
            for &(t, c) in costs {
                self.costs[t as usize] = c;
            }
            self.diagonal = diagonal;
            self.manhattan = false;
            self.check_blocked = false;
        }
    }

    /// `FUN_1400ab390`: neighbour `dir` of `cell`, if inside, unblocked and allowed.
    fn neighbour(&self, cell: i32, dir: usize) -> Option<i32> {
        let n = self.size;
        let (mut x, mut y) = (cell % n, cell / n);
        match dir {
            0 => x += 1,
            1 => x -= 1,
            2 => y += 1,
            3 => y -= 1,
            4 => (x, y) = (x + 1, y + 1),
            5 => (x, y) = (x + 1, y - 1),
            6 => (x, y) = (x - 1, y - 1),
            _ => (x, y) = (x - 1, y + 1),
        }
        if x < 0 || x >= n || y < 0 || y >= n {
            return None;
        }
        let k = n * y + x;
        let byte = self.cells[k as usize];
        if self.check_blocked && byte & 0x80 != 0 {
            return None;
        }
        (self.mask >> (byte & 0x1f) & 1 != 0).then_some(k)
    }

    /// `FUN_1400ab500`.
    fn heuristic(&self, a: i32, b: i32) -> i32 {
        let n = self.size;
        let dx = (a % n - b % n).abs();
        let dy = (a / n - b / n).abs();
        if self.diagonal {
            (dy + dx) * 10 - 6 * dx.min(dy)
        } else if self.manhattan {
            (dy + dx) * 10
        } else {
            dy.max(dx) * 10
        }
    }

    /// `FUN_14001dc80`: open (or reopen) `cell` with a new parent and cost.
    fn push(&mut self, cell: i32, parent: i32, g: i32, f: i32) {
        let node = self.nodes[cell as usize];
        if node.open == self.search {
            self.open.remove(&((node.key.0, Reverse(node.key.1)), cell));
        }
        let counter = self.counter;
        self.counter += 1;
        let node = &mut self.nodes[cell as usize];
        node.closed = node.closed.wrapping_sub(1);
        node.parent = parent;
        node.open = self.search;
        node.g = g;
        node.key = (f, counter);
        self.open.insert(((f, Reverse(counter)), cell));
    }

    /// `FUN_14001d9a0`: A* from `from` to `to`. The open set orders by `f`, then by the most
    /// recent insertion (the AVL comparator at `FUN_14001ddb0`).
    fn solve(&mut self, from: i32, to: i32) -> bool {
        self.open.clear();
        self.counter = 0;
        let h = self.heuristic(from, to);
        self.push(from, -1, 0, h);
        let n = self.size;
        while let Some(&first) = self.open.first() {
            let cur = first.1;
            if cur == to {
                return true;
            }
            self.open.remove(&first);
            let search = self.search;
            let node = &mut self.nodes[cur as usize];
            node.open = node.open.wrapping_sub(1);
            node.closed = search;
            let g_cur = node.g;
            for dir in 0..if self.diagonal { 8 } else { 4 } {
                let Some(nb) = self.neighbour(cur, dir) else {
                    continue;
                };
                let straight = (cur % n - nb % n).abs() + (cur / n - nb / n).abs() == 1;
                let cost = self.costs[(self.cells[nb as usize] & 0x7f) as usize & 31]
                    * if straight { 10 } else { 14 };
                let g = g_cur + cost;
                let node = &mut self.nodes[nb as usize];
                if node.closed == search {
                    if node.g <= g {
                        continue;
                    }
                    node.closed = node.closed.wrapping_sub(1);
                }
                if node.open != search || g < node.g {
                    let f = self.heuristic(nb, to) + g;
                    self.push(nb, cur, g, f);
                }
            }
        }
        false
    }

    /// `FUN_14001d760`: the cells from `from` to `to` inclusive, or `None`.
    pub fn find(&mut self, from: i32, to: i32) -> Option<Vec<i32>> {
        self.search = self.search.wrapping_add(1);
        if !self.solve(from, to) {
            return None;
        }
        let mut path = Vec::new();
        let mut c = to;
        while c != -1 {
            path.push(c);
            c = self.nodes[c as usize].parent;
        }
        path.reverse();
        path.truncate(MAX_PATH);
        Some(path)
    }
}

/// `FUN_1400a2480`: validate a request (both end types join the mask; without water types both
/// ends must share a non-zero region), set the mask and search.
pub fn request(
    finder: &mut Pathfinder,
    types: &[CellType],
    region: &[u16],
    flags: u32,
    from: i32,
    to: i32,
) -> Option<Vec<i32>> {
    let (a, b) = (bit(types[from as usize]), bit(types[to as usize]));
    let mut flags = flags | a | b;
    if flags & WATER == 0 {
        let (ra, rb) = (region[from as usize], region[to as usize]);
        if ra == 0 || rb == 0 || ra != rb {
            return None;
        }
    }
    if flags & 0x180 != 0 {
        flags |= 0x180;
    }
    finder.mask = flags;
    finder.find(from, to)
}

/// `FUN_1400a21c0`: of the 8 cells around `cell` whose type is in `mask`, the one nearest
/// (squared cell distance) to `toward`; the first found wins ties (x outer, y inner).
pub fn nearest_neighbour(
    size: usize,
    types: &[CellType],
    cell: i32,
    toward: i32,
    mask: u32,
) -> Option<i32> {
    let n = size as i32;
    let (cx, cy) = (cell % n, cell / n);
    let mut best: Option<(i32, i32)> = None;
    for dx in -1..=1 {
        for dy in -1..=1 {
            let (x, y) = (cx + dx, cy + dy);
            if (dx == 0 && dy == 0) || x < 0 || y < 0 || x >= n || y >= n {
                continue;
            }
            let k = n * y + x;
            if mask & bit(types[k as usize]) == 0 {
                continue;
            }
            let (ex, ey) = (k / n - toward / n, k % n - toward % n);
            let d = ex * ex + ey * ey;
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((k, d));
            }
        }
    }
    best.map(|(k, _)| k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&str]) -> (usize, Vec<CellType>) {
        // Rows are given top (high y) first; '.' ground, '#' water, '=' road.
        let n = rows.len();
        let mut t = vec![CellType(2); n * n];
        for (r, row) in rows.iter().enumerate() {
            let y = n - 1 - r;
            for (x, ch) in row.chars().enumerate() {
                t[y * n + x] = match ch {
                    '#' => CellType(8),
                    '=' => CellType(1),
                    _ => CellType(2),
                };
            }
        }
        (n, t)
    }

    #[test]
    fn regions_split_by_water() {
        let (n, t) = grid(&["..#..", "..#..", "..#..", "..#..", "..#.."]);
        let r = regions(n, &t);
        assert_eq!(r[0], 1);
        assert_eq!(r[3], 2);
        assert_eq!(r[2], 0);
    }

    #[test]
    fn path_goes_around_water_and_prefers_roads() {
        let (n, t) = grid(&[".....", ".###.", ".....", ".....", "....."]);
        let r = regions(n, &t);
        let mut pf = Pathfinder::new(n, &t);
        pf.set_mode(0);
        let from = (3 * n) as i32; // (0, 3)
        let to = (3 * n + 4) as i32; // (4, 3), across the water row
        let p = request(&mut pf, &t, &r, WALKABLE, from, to).unwrap();
        assert_eq!(*p.first().unwrap(), from);
        assert_eq!(*p.last().unwrap(), to);
        assert!(p.iter().all(|&c| t[c as usize] != CellType(8)));
        // Unreachable across a full water column.
        let (n, t) = grid(&["..#..", "..#..", "..#..", "..#..", "..#.."]);
        let r = regions(n, &t);
        let mut pf = Pathfinder::new(n, &t);
        pf.set_mode(0);
        assert!(request(&mut pf, &t, &r, WALKABLE, 0, 4).is_none());
    }
}

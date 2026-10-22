//! The sector graph, and the "path rank" distance that storage, job and resource searches use.
//!
//! Running a full A* search for every candidate storage or job would be slow. So the game cuts
//! the map into 8 × 8-cell sectors. The walkable cells of each sector split into connected
//! areas, and each area becomes a node at the mean of its cells. Nodes link to the nodes just
//! outside their sector. A distance is an A* search over the nodes, with Euclidean costs
//! truncated to integers, and it is cached for 120 seconds of game time.
//!
//! When the map changes, [`SectorGraph::mark`] marks the sector, and the graph rebuilds the
//! marked sectors before the next search.

mod search;

// Original: FUN_1400a4ce0 (path rank); sectors FUN_1400adb40, areas FUN_1400ad3f0 /
// FUN_1400acfa0, nodes FUN_1400ace20, cell node (cell +0x18) FUN_1400acb40, links
// FUN_1400acc20 / FUN_1400ac950, marking FUN_1400ae160, lazy rebuild FUN_1400adf60, node A*
// FUN_14001e630, summed by FUN_1400ae070, cache FUN_1400ab000 / FUN_1400ab1e0.
// See docs/CITIZEN-AI.md.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::pathfind::WALKABLE;
use unbanished_terrain::CellType;

const SECTOR: i32 = 8;
/// `DAT_140178910`: cached distances live 120 s.
const CACHE_LIFETIME: i64 = 120_000_000;
/// `FUN_14001d600`: cache ring size.
const CACHE_SIZE: usize = 0x20000;
/// `FUN_14001e560`: longest node path.
const MAX_NODES: usize = 0x2000;

fn walk(t: CellType) -> bool {
    (1u32 << (t.0 & 0x1f)) & WALKABLE != 0
}

/// `(int) sqrtf((float) (dx² + dy²))`.
fn length(a: (i32, i32), b: (i32, i32)) -> i32 {
    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
    ((dx * dx + dy * dy) as f32).sqrt() as i32
}

#[derive(Debug, Clone, Default)]
struct Node {
    /// `+0`, `+4`: position (mean cell); `+8`: sector.
    pos: (i32, i32),
    #[allow(dead_code)]
    sector: i32,
    /// `+0x10`: linked nodes; `+0x20`: cells just outside the sector next to this area.
    links: Vec<i32>,
    border: Vec<i32>,
}

#[derive(Debug, Clone, Default)]
struct Sector {
    x: i32,
    y: i32,
    nodes: Vec<i32>,
    node_dirty: bool,
    link_dirty: bool,
}

#[derive(Clone, Copy, Default)]
struct Search {
    parent: i32,
    open: i32,
    closed: i32,
    g: i32,
    key: (i32, i32),
}

/// `FUN_1400ab000`'s cache: a FIFO ring keyed by node pair.
#[derive(Debug, Clone)]
struct Cache {
    time: i64,
    head: usize,
    tail: usize,
    /// `(key, value, time)` or `None` for a cleared slot.
    ring: Vec<Option<((i32, i32), i32, i64)>>,
    index: BTreeMap<(i32, i32), usize>,
}

/// The sector graph: the map cut into 8 × 8-cell sectors, the walkable areas of each sector as
/// nodes, and links between neighbouring areas. It gives the "path rank" distance that storage,
/// job and resource searches use, and caches it.
pub struct SectorGraph {
    n: i32,
    w: i32,
    h: i32,
    nodes: Vec<Node>,
    sectors: Vec<Sector>,
    dirty_nodes: Vec<usize>,
    dirty_links: Vec<usize>,
    free: Vec<i32>,
    // +0x18
    /// Cell: node of each cell (-1 none).
    pub cell_node: Vec<i32>,
    search: Vec<Search>,
    search_id: i32,
    open: BTreeSet<((i32, Reverse<i32>), i32)>,
    counter: i32,
    cache: Cache,
}

impl SectorGraph {
    /// The graph of a map: every sector is built, then linked, in index order.
    ///
    /// Original: `FUN_1400adb40`, then `FUN_1400adf60`.
    pub fn new(size: usize, types: &[CellType]) -> SectorGraph {
        let n = size as i32;
        let (w, h) = (n / SECTOR, n / SECTOR);
        let mut sectors = Vec::new();
        for y in 0..h {
            for x in 0..w {
                sectors.push(Sector {
                    x,
                    y,
                    nodes: Vec::new(),
                    node_dirty: true,
                    link_dirty: true,
                });
            }
        }
        let count = sectors.len();
        let mut g = SectorGraph {
            n,
            w,
            h,
            nodes: Vec::new(),
            sectors,
            dirty_nodes: (0..count).collect(),
            dirty_links: (0..count).collect(),
            free: Vec::new(),
            cell_node: vec![-1; size * size],
            search: Vec::new(),
            search_id: 1,
            open: BTreeSet::new(),
            counter: 0,
            cache: Cache {
                time: 0,
                head: 0,
                tail: 0,
                ring: vec![None; CACHE_SIZE],
                index: BTreeMap::new(),
            },
        };
        g.rebuild(types);
        g
    }

    /// A cell changed; rebuild its sector and relink the 3 × 3 around it.
    ///
    /// Original: `FUN_1400ae160`.
    pub fn mark(&mut self, x: i32, y: i32) {
        let s = ((y / SECTOR) * self.w + x / SECTOR) as usize;
        if self.sectors[s].node_dirty {
            return;
        }
        self.sectors[s].node_dirty = true;
        self.dirty_nodes.push(s);
        let (sx, sy) = (self.sectors[s].x, self.sectors[s].y);
        for yy in sy - 1..=sy + 1 {
            if yy < 0 || yy >= self.h {
                continue;
            }
            for xx in sx - 1..=sx + 1 {
                if xx < 0 || xx >= self.w {
                    continue;
                }
                let t = (yy * self.w + xx) as usize;
                if !self.sectors[t].link_dirty {
                    self.sectors[t].link_dirty = true;
                    self.dirty_links.push(t);
                }
            }
        }
    }

    /// Rebuild and relink the sectors that [`mark`](Self::mark) marked.
    ///
    /// Original: `FUN_1400adf60`.
    pub fn rebuild(&mut self, types: &[CellType]) {
        if self.dirty_nodes.is_empty() && self.dirty_links.is_empty() {
            return;
        }
        for s in std::mem::take(&mut self.dirty_nodes) {
            self.build_sector(s, types);
            self.sectors[s].node_dirty = false;
        }
        for s in std::mem::take(&mut self.dirty_links) {
            self.link_sector(s);
            self.sectors[s].link_dirty = false;
        }
        self.search = vec![Search::default(); self.nodes.len()];
    }

    /// `FUN_1400ad570`.
    fn build_sector(&mut self, s: usize, types: &[CellType]) {
        // `FUN_1400ad6d0`: free the sector's nodes.
        for id in std::mem::take(&mut self.sectors[s].nodes) {
            let node = &mut self.nodes[id as usize];
            *node = Node {
                pos: (-1, -1),
                sector: -1,
                ..Default::default()
            };
            self.free.push(id);
        }
        let (x0, y0) = (self.sectors[s].x * SECTOR, self.sectors[s].y * SECTOR);
        let labels = self.label(x0, y0, types);
        let count = labels.iter().copied().max().unwrap_or(0);
        for label in 1..=count {
            let id = self.create_node(s, x0, y0, &labels, label);
            self.sectors[s].nodes.push(id);
        }
        // `FUN_1400acb40`.
        for ly in 0..SECTOR {
            for lx in 0..SECTOR {
                let label = labels[(ly * SECTOR + lx) as usize];
                let cell = ((y0 + ly) * self.n + x0 + lx) as usize;
                self.cell_node[cell] = if label >= 1 {
                    self.sectors[s].nodes[label as usize - 1]
                } else {
                    -1
                };
            }
        }
    }

    /// `FUN_1400ad3f0` / `FUN_1400acfa0`: label the sector's connected walkable areas (rows
    /// outer) with an 8-connected FIFO scanline fill.
    fn label(&self, x0: i32, y0: i32, types: &[CellType]) -> Vec<i32> {
        let mut labels = vec![0i32; (SECTOR * SECTOR) as usize];
        let at = |lx: i32, ly: i32| types[((y0 + ly) * self.n + x0 + lx) as usize];
        let mut next = 1;
        for ly in 0..SECTOR {
            for lx in 0..SECTOR {
                if !walk(at(lx, ly)) || labels[(ly * SECTOR + lx) as usize] != 0 {
                    continue;
                }
                let mut queue = VecDeque::from([(lx, ly)]);
                while let Some((qx, qy)) = queue.pop_front() {
                    if labels[(qy * SECTOR + qx) as usize] != 0 || !walk(at(qx, qy)) {
                        continue;
                    }
                    let mut left = qx;
                    while left - 1 >= 0 && walk(at(left - 1, qy)) {
                        left -= 1;
                    }
                    let mut right = qx;
                    while right + 1 <= SECTOR - 1 && walk(at(right + 1, qy)) {
                        right += 1;
                    }
                    for u in left..=right {
                        labels[(qy * SECTOR + u) as usize] = next;
                    }
                    if left - 1 >= 0 {
                        left -= 1;
                    }
                    if right + 1 <= SECTOR - 1 {
                        right += 1;
                    }
                    for (ok, row) in [(qy > 0, qy - 1), (qy < SECTOR - 1, qy + 1)] {
                        if !ok {
                            continue;
                        }
                        for u in left..=right {
                            if labels[(row * SECTOR + u) as usize] == 0 && walk(at(u, row)) {
                                queue.push_back((u, row));
                            }
                        }
                    }
                }
                next += 1;
            }
        }
        labels
    }

    /// `FUN_1400ace20`: a node for one labelled area.
    fn create_node(&mut self, s: usize, x0: i32, y0: i32, labels: &[i32], label: i32) -> i32 {
        // `FUN_1400ad820`: reuse the most recently freed id.
        let id = match self.free.pop() {
            Some(id) => id,
            None => {
                self.nodes.push(Node::default());
                self.nodes.len() as i32 - 1
            }
        };
        let (mut sx, mut sy, mut count) = (0, 0, 0);
        let mut border = Vec::new();
        for ly in 0..SECTOR {
            for lx in 0..SECTOR {
                if labels[(ly * SECTOR + lx) as usize] != label {
                    continue;
                }
                let (gx, gy) = (lx + x0, ly + y0);
                count += 1;
                sx += gx;
                sy += gy;
                if lx == 0 || ly == 0 || lx == SECTOR - 1 || ly == SECTOR - 1 {
                    // `FUN_1400acc20`: neighbours outside the sector.
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let (cx, cy) = (gx + dx, gy + dy);
                            if cx < 0 || cy < 0 || cx >= self.n || cy >= self.n {
                                continue;
                            }
                            let outside =
                                cx < x0 || cy < y0 || cx > x0 + SECTOR - 1 || cy > y0 + SECTOR - 1;
                            let cell = cy * self.n + cx;
                            if outside && !border.contains(&cell) {
                                border.push(cell);
                            }
                        }
                    }
                }
            }
        }
        self.nodes[id as usize] = Node {
            pos: (sx / count, sy / count),
            sector: s as i32,
            links: Vec::new(),
            border,
        };
        id
    }

    /// `FUN_1400ac950`.
    fn link_sector(&mut self, s: usize) {
        for &id in &self.sectors[s].nodes.clone() {
            let border = self.nodes[id as usize].border.clone();
            let mut links = Vec::new();
            for cell in border {
                let other = self.cell_node[cell as usize];
                if other >= 0 && !links.contains(&other) {
                    links.push(other);
                }
            }
            self.nodes[id as usize].links = links;
        }
    }

    /// The node of `cell`, else the first node found on rings 1–3 around it.
    ///
    /// Original: `FUN_1400a2570`.
    pub fn node_near(&self, cell: i32) -> i32 {
        let n = self.n;
        let id = self.cell_node[cell as usize];
        if id >= 0 {
            return id;
        }
        let (x, y) = (cell % n, cell / n);
        let node = |cx: i32, cy: i32| -> Option<i32> {
            (cx >= 0 && cy >= 0 && cx < n && cy < n)
                .then(|| self.cell_node[(cy * n + cx) as usize])
                .filter(|&v| v >= 0)
        };
        for r in 1..=3 {
            for t in x - r..=x + r {
                let row = y + (t - x);
                if row >= 0 && row < n {
                    if let Some(v) = node(x - r, row).or_else(|| node(x + r, row)) {
                        return v;
                    }
                }
                if t >= 0 && t < n {
                    if let Some(v) = node(t, y - r).or_else(|| node(t, y + r)) {
                        return v;
                    }
                }
            }
        }
        -1
    }
}

#[cfg(test)]
mod tests;

//! The map's coarse path graph and the path-rank distance every storage and job query uses
//! (`FUN_1400a4ce0`). See docs/CITIZEN-AI.md.
//!
//! The map is cut into 8 × 8-cell sectors (`FUN_1400adb40`, `n / 8` per side). Each sector's
//! walkable cells split into connected areas (`FUN_1400ad3f0` / `FUN_1400acfa0`), each a node
//! at the integer mean of its cells (`FUN_1400ace20`); every cell records its node (cell
//! `+0x18`, `FUN_1400acb40`). Nodes link to the nodes of the cells just outside their sector
//! border (`FUN_1400acc20`, `FUN_1400ac950`). Map changes mark sectors for rebuilding
//! (`FUN_1400ae160`), done lazily before a search (`FUN_1400adf60`). Distances are A* over
//! nodes with truncated Euclidean costs (`FUN_14001e630`), summed (`FUN_1400ae070`) and cached
//! for 120 s (`FUN_1400ab000`, `FUN_1400ab1e0`).

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::pathfind::WALKABLE;
use crate::terrain::CellType;

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

pub struct SectorGraph {
    n: i32,
    w: i32,
    h: i32,
    nodes: Vec<Node>,
    sectors: Vec<Sector>,
    dirty_nodes: Vec<usize>,
    dirty_links: Vec<usize>,
    free: Vec<i32>,
    /// Cell `+0x18`: node of each cell (-1 none).
    pub cell_node: Vec<i32>,
    search: Vec<Search>,
    search_id: i32,
    open: BTreeSet<((i32, Reverse<i32>), i32)>,
    counter: i32,
    cache: Cache,
}

impl SectorGraph {
    /// `FUN_1400adb40` then `FUN_1400adf60`: every sector is built, then linked, in index order.
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

    /// `FUN_1400ae160`: a cell changed; rebuild its sector and relink the 3 × 3 around it.
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

    /// `FUN_1400adf60`.
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

    /// `FUN_1400a2570`: the node of `cell`, else the first node found on rings 1–3 around it.
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

    fn push(&mut self, node: i32, parent: i32, g: i32, f: i32) {
        let s = self.search[node as usize];
        if s.open == self.search_id {
            self.open.remove(&((s.key.0, Reverse(s.key.1)), node));
        }
        let counter = self.counter;
        self.counter += 1;
        let s = &mut self.search[node as usize];
        s.closed = s.closed.wrapping_sub(1);
        s.parent = parent;
        s.open = self.search_id;
        s.g = g;
        s.key = (f, counter);
        self.open.insert(((f, Reverse(counter)), node));
    }

    /// `FUN_14001e560` / `FUN_14001e630`: node path from `a` to `b`.
    fn node_path(&mut self, a: i32, b: i32) -> Option<Vec<i32>> {
        self.search_id = self.search_id.wrapping_add(1);
        if self.search.len() < self.nodes.len() {
            self.search.resize(self.nodes.len(), Search::default());
        }
        self.open.clear();
        self.counter = 0;
        let h = length(self.nodes[b as usize].pos, self.nodes[a as usize].pos);
        self.push(a, -1, 0, h);
        let mut found = false;
        while let Some(&first) = self.open.first() {
            let cur = first.1;
            if cur == b {
                found = true;
                break;
            }
            self.open.remove(&first);
            let id = self.search_id;
            let s = &mut self.search[cur as usize];
            s.open = s.open.wrapping_sub(1);
            s.closed = id;
            let g_cur = s.g;
            let pos = self.nodes[cur as usize].pos;
            for k in 0..self.nodes[cur as usize].links.len() {
                let nb = self.nodes[cur as usize].links[k];
                if nb < 0 {
                    continue;
                }
                let npos = self.nodes[nb as usize].pos;
                let g = length(npos, pos) + g_cur;
                let s = &mut self.search[nb as usize];
                if s.closed == id {
                    if s.g <= g {
                        continue;
                    }
                    s.closed = s.closed.wrapping_sub(1);
                }
                if s.open != id || g < s.g {
                    let f = length(self.nodes[b as usize].pos, npos) + g;
                    self.push(nb, cur, g, f);
                }
            }
        }
        if !found {
            return None;
        }
        let mut path = Vec::new();
        let mut c = b;
        while c != -1 {
            path.push(c);
            c = self.search[c as usize].parent;
        }
        path.reverse();
        path.truncate(MAX_NODES);
        Some(path)
    }

    /// `FUN_1400ae070`: summed hop lengths of the node path (0 for fewer than 2 nodes).
    fn node_distance(&mut self, a: i32, b: i32, types: &[CellType]) -> i32 {
        self.rebuild(types);
        let Some(path) = self.node_path(a, b) else {
            return 0;
        };
        path.windows(2)
            .map(|w| length(self.nodes[w[1] as usize].pos, self.nodes[w[0] as usize].pos))
            .sum()
    }

    /// `FUN_1400a4ce0`: path rank from cell `a` to cell `b`, or -1 when unreachable.
    /// `reachable` is `FUN_1400a55c0(map, 0xe1f, a, b)`; `store` caches a computed distance.
    pub fn distance(
        &mut self,
        a: i32,
        b: i32,
        reachable: bool,
        store: bool,
        types: &[CellType],
    ) -> i32 {
        if !reachable {
            return -1;
        }
        let na = self.node_near(a);
        let nb = self.node_near(b);
        if na < 0 || nb < 0 {
            return -1;
        }
        if na == nb {
            return 1;
        }
        let key = (na.max(nb), na.min(nb));
        if let Some(&slot) = self.cache.index.get(&key) {
            if let Some((_, v, _)) = self.cache.ring[slot] {
                return v;
            }
        }
        let d = self.node_distance(na, nb, types);
        if store {
            self.cache_store(key, d);
        }
        d
    }

    /// `FUN_1400ab000`.
    fn cache_store(&mut self, key: (i32, i32), value: i32) {
        if let Some(&slot) = self.cache.index.get(&key) {
            if let Some(e) = &mut self.cache.ring[slot] {
                e.1 = value;
                return;
            }
        }
        let slot = self.cache.head;
        self.cache.head = (slot + 1) % CACHE_SIZE;
        if self.cache.head == self.cache.tail {
            self.evict_oldest();
        }
        self.cache.ring[slot] = Some((key, value, self.cache.time));
        self.cache.index.insert(key, slot);
    }

    fn evict_oldest(&mut self) {
        let t = self.cache.tail;
        if let Some((k, _, _)) = self.cache.ring[t].take() {
            if self.cache.index.get(&k) == Some(&t) {
                self.cache.index.remove(&k);
            }
        }
        self.cache.tail = (t + 1) % CACHE_SIZE;
    }

    /// `FUN_1400ab1e0`, once per simulation iteration: age the cache, dropping entries older
    /// than 120 s from the oldest end.
    pub fn age_cache(&mut self, dt: i64) {
        self.cache.time += dt;
        loop {
            let t = self.cache.tail;
            let entry_time = self.cache.ring[t].map_or(0, |e| e.2);
            if !(CACHE_LIFETIME < self.cache.time - entry_time) {
                return;
            }
            if t == self.cache.head {
                return;
            }
            self.evict_oldest();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_map_distances() {
        let n = 32;
        let types = vec![CellType(2); n * n];
        let mut g = SectorGraph::new(n, &types);
        // One node per sector at its centre.
        assert_eq!(g.cell_node[0], g.cell_node[7 * n + 7]);
        assert_ne!(g.cell_node[0], g.cell_node[8]);
        assert_eq!(g.distance(0, 1, true, false, &types), 1);
        let d = g.distance(0, (n * n - 1) as i32, true, true, &types);
        assert!(d > 0);
        assert_eq!(g.distance(0, (n * n - 1) as i32, true, false, &types), d);
    }
}

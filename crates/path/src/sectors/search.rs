//! Searches over the sector graph: the open list, node paths and distances, and the distance cache.

use super::*;

impl SectorGraph {
    pub(super) fn push(&mut self, node: i32, parent: i32, g: i32, f: i32) {
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
    pub(super) fn node_path(&mut self, a: i32, b: i32) -> Option<Vec<i32>> {
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
    pub(super) fn node_distance(&mut self, a: i32, b: i32, types: &[CellType]) -> i32 {
        self.rebuild(types);
        let Some(path) = self.node_path(a, b) else {
            return 0;
        };
        path.windows(2)
            .map(|w| length(self.nodes[w[1] as usize].pos, self.nodes[w[0] as usize].pos))
            .sum()
    }

    /// The path-rank distance from cell `a` to cell `b`, or -1 when `b` cannot be reached.
    /// `reachable` is the caller's region test (both cells in the same walkable region); `store`
    /// keeps a computed distance in the cache.
    ///
    /// Original: `FUN_1400a4ce0`.
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
    pub(super) fn cache_store(&mut self, key: (i32, i32), value: i32) {
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

    pub(super) fn evict_oldest(&mut self) {
        let t = self.cache.tail;
        if let Some((k, _, _)) = self.cache.ring[t].take() {
            if self.cache.index.get(&k) == Some(&t) {
                self.cache.index.remove(&k);
            }
        }
        self.cache.tail = (t + 1) % CACHE_SIZE;
    }

    /// Age the cache by `dt` ticks, and drop the entries older than 120 seconds. The game calls it
    /// once per simulation iteration.
    ///
    /// Original: `FUN_1400ab1e0`.
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

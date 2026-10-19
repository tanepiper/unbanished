//! The characters' spatial tree: a loose quadtree of the CharacterComponents, which the
//! tornado's victims and the hunters' prey are searched in.
//!
//! - **Built**: a world of 2048 units centred on the map (offset 1024, 1024), 8
//!   levels, node `base[level] + Morton(x, y)` with the bases 0, 1, 5, 0x15, … 0x1555 (0x5555
//!   nodes). Each node keeps a circular list of its slots (the head) and the count of slots in it
//!   and below (carried up `parent = (i − 1) >> 2`).
//! - **Insert** (when a character starts or is loaded): the last freed slot, else a new one; then
//!   placed.
//! - **Remove** (when a character ends): unlinked, the slot freed.
//! - **Place** (at every character update): the level from the character's
//!   square (its position ± the description's radius), the cell its centre falls in at that level;
//!   a slot that changes node goes to the end of the new node's list.
//! - **Search**: a stack from the root; a node with nothing in or below it, or whose loose box (its
//!   square doubled, z from −1 to 1) misses the query's box (the point ± the radius on every axis),
//!   is dropped; otherwise its slots are visited from the head and it is replaced by its four
//!   children (pushed 1 to 4, so the fourth comes off first). The z bounds mean nothing is found
//!   while the point stands more than the radius + 1 from height 0.
//!
//! Original: built by `FUN_140018cc0`; insert `FUN_14010ee90` (start) and `FUN_14010ef80` (load);
//! remove `FUN_14010ee50`; place `FUN_14010e990`; and `FUN_140097e60`, `FUN_140097730`, `FUN_140097d50`, `FUN_140018e00`, `FUN_140018f70`, `FUN_140019070`, `FUN_140019710`, `FUN_140019650`, `FUN_1400197e0`, `FUN_140019290`.
// Game +0x88; +0x30; +0x50; +0x20

/// `DAT_140178958`: the world's side.
const WORLD: f32 = 2048.0;
/// `FUN_140018cc0`: the levels and the first node of each.
const LEVELS: i32 = 8;
const BASES: [i32; 8] = [0, 1, 5, 0x15, 0x55, 0x155, 0x555, 0x1555];
const NODES: usize = 0x5555;

/// What a slot holds: the owner of the CharacterComponent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    /// A citizen.
    Citizen(usize),
    /// An animal.
    Animal(usize),
    /// A trader boat.
    Boat(usize),
    /// A nomad.
    Nomad(usize),
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    who: Option<Who>,
    /// `+0x08`, `+0x0c`: the node list's next and previous slots.
    next: i32,
    prev: i32,
    /// `+0x10`: the node, −1 when in none.
    node: i32,
    position: [f32; 3],
    /// The component's walking direction (`+0x2c`), as of its last update.
    direction: [f32; 3],
    /// When it was put in, standing in for the component's address in `FUN_1400cde40`.
    serial: u64,
}

/// The characters' spatial tree. See the [module documentation](self).
#[derive(Debug, Clone)]
pub struct CharacterTree {
    slots: Vec<Slot>,
    free: Vec<i32>,
    heads: Vec<i32>,
    counts: Vec<i32>,
    serials: u64,
}

impl Default for CharacterTree {
    fn default() -> Self {
        CharacterTree {
            slots: Vec::new(),
            free: Vec::new(),
            heads: vec![-1; NODES],
            counts: vec![0; NODES],
            serials: 0,
        }
    }
}

/// `FUN_140019070`'s bit interleave: x in the even bits, y in the odd.
fn morton(x: u32, y: u32) -> u32 {
    let spread = |mut v: u32| {
        v = (v << 8 | v) & 0x00ff_00ff;
        v = (v << 4 | v) & 0x0f0f_0f0f;
        v = (v << 2 | v) & 0x3333_3333;
        (v << 1 | v) & 0x5555_5555
    };
    spread(x) | spread(y) << 1
}

/// The node a character at `position` with `radius` belongs in (`FUN_140019070`).
fn node_of(position: [f32; 3], radius: f32) -> i32 {
    // FUN_140097d90: the square position ± radius; its centre and half its size.
    let (x0, y0) = (position[0] - radius, position[1] - radius);
    let (x1, y1) = (radius + position[0], radius + position[1]);
    let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let half = ((x1 - x0) * 0.5).max((y1 - y0) * 0.5);
    let q = WORLD / (half + 0.001);
    let level = ((q.to_bits() >> 23) as i32 - 127).clamp(0, LEVELS - 1);
    let side = 1i32 << level;
    let scale = 1.0 / (WORLD / side as f32);
    let (cx, cy) = (cx + WORLD * 0.5, cy + WORLD * 0.5);
    let x = ((cx * scale) as i32).clamp(0, side - 1) as u32;
    let y = ((cy * scale) as i32).clamp(0, side - 1) as u32;
    morton(x, y) as i32 + BASES[level as usize]
}

impl CharacterTree {
    /// A slot for `who`, placed at once. Returns the slot.
    ///
    /// Original: `FUN_140018e00`.
    pub fn insert(&mut self, who: Who, position: [f32; 3], radius: f32) -> i32 {
        self.serials += 1;
        let blank = Slot {
            who: Some(who),
            next: 0,
            prev: 0,
            node: -1,
            position,
            direction: [0.0; 3],
            serial: self.serials,
        };
        let slot = match self.free.pop() {
            Some(s) => {
                self.slots[s as usize] = blank;
                s
            }
            None => {
                self.slots.push(blank);
                self.slots.len() as i32 - 1
            }
        };
        self.place(slot, position, radius);
        slot
    }

    /// Out of its node; the slot goes on the free list. The slot holding `who`, if it is in the
    /// tree.
    ///
    /// Original: `FUN_140018f70`.
    pub fn slot_of(&self, who: Who) -> Option<i32> {
        self.slots
            .iter()
            .position(|s| s.who == Some(who))
            .map(|i| i as i32)
    }

    /// Remove a character's slot.
    pub fn remove(&mut self, slot: i32) {
        if slot < 0
            || self
                .slots
                .get(slot as usize)
                .is_none_or(|s| s.who.is_none())
        {
            return;
        }
        if 0 <= self.slots[slot as usize].node {
            self.unlink(slot);
        }
        let s = &mut self.slots[slot as usize];
        s.who = None;
        s.next = 0;
        s.node = -1;
        self.free.push(slot);
    }

    /// Where the character stands now; a new node puts it at the end of that node's list.
    ///
    /// Original: `FUN_140019070`.
    pub fn place(&mut self, slot: i32, position: [f32; 3], radius: f32) {
        if slot < 0
            || self
                .slots
                .get(slot as usize)
                .is_none_or(|s| s.who.is_none())
        {
            return;
        }
        self.slots[slot as usize].position = position;
        let node = node_of(position, radius);
        if self.slots[slot as usize].node != node {
            if 0 <= self.slots[slot as usize].node {
                self.unlink(slot);
            }
            self.slots[slot as usize].node = node;
            self.link(slot);
        }
    }

    // +0x2c
    /// The walking direction the follow check reads (the component's).
    pub fn set_direction(&mut self, slot: i32, direction: [f32; 3]) {
        if let Some(s) = usize::try_from(slot)
            .ok()
            .and_then(|i| self.slots.get_mut(i))
        {
            s.direction = direction;
        }
    }

    // its slot +0x30
    /// MoveTo's notify: the nearest other character within 0.5, if it came after this one (the
    /// original compares the components' addresses; the port, the order they were put in the tree)
    /// and walks the same way (the directions' dot product above 0), is followed.
    ///
    /// Original: `FUN_1400cde40`, `FUN_140097730`.
    pub fn follows(&self, slot: i32) -> bool {
        let Some(me) = usize::try_from(slot).ok().and_then(|i| self.slots.get(i)) else {
            return false;
        };
        let Some(who) = me.who else { return false };
        let mut best: Option<&Slot> = None;
        let mut most = 0.5f32 * 0.5;
        self.visit_slots(me.position, 0.5, |s| {
            if s.who != Some(who) {
                let d = dist2(s.position, me.position);
                if d < most {
                    most = d;
                    best = Some(s);
                }
            }
        });
        let Some(other) = best else { return false };
        let dot = other.direction[0] * me.direction[0]
            + other.direction[1] * me.direction[1]
            + other.direction[2] * me.direction[2];
        // Deviation: the original follows a character whose component sits at a higher
        // address; the port takes the one put in the tree later.
        me.serial < other.serial && 0.0 < dot
    }

    /// `FUN_140019650`: at the end of its node's circular list.
    fn link(&mut self, slot: i32) {
        let node = self.slots[slot as usize].node as usize;
        let head = self.heads[node];
        if head < 0 {
            self.heads[node] = slot;
            self.slots[slot as usize].next = slot;
            self.slots[slot as usize].prev = slot;
        } else {
            let last = self.slots[head as usize].prev;
            self.slots[slot as usize].prev = last;
            self.slots[slot as usize].next = head;
            self.slots[last as usize].next = slot;
            self.slots[head as usize].prev = slot;
        }
        let mut n = node as i32;
        self.counts[n as usize] += 1;
        while n != 0 {
            n = (n - 1) >> 2;
            self.counts[n as usize] += 1;
        }
    }

    /// `FUN_140019710`.
    fn unlink(&mut self, slot: i32) {
        let node = self.slots[slot as usize].node as usize;
        if self.heads[node] == slot {
            self.heads[node] = self.slots[slot as usize].next;
        }
        let (next, prev) = (
            self.slots[slot as usize].next,
            self.slots[slot as usize].prev,
        );
        self.slots[prev as usize].next = next;
        self.slots[next as usize].prev = prev;
        self.slots[slot as usize].next = -1;
        self.slots[slot as usize].prev = -1;
        if self.heads[node] == slot {
            self.heads[node] = -1;
        }
        let mut n = node as i32;
        self.counts[n as usize] -= 1;
        while n != 0 {
            n = (n - 1) >> 2;
            self.counts[n as usize] -= 1;
        }
    }

    /// The search: every slot of the nodes whose loose box meets `point ± radius`, in the
    /// original's order, with where it stands.
    ///
    /// Original: `FUN_1400197e0`, `FUN_140019290`.
    pub fn visit(&self, point: [f32; 3], radius: f32, mut each: impl FnMut(Who, [f32; 3])) {
        self.visit_slots(point, radius, |s| {
            if let Some(who) = s.who {
                each(who, s.position);
            }
        });
    }

    fn visit_slots<'a>(&'a self, point: [f32; 3], radius: f32, mut each: impl FnMut(&'a Slot)) {
        let qmin = [point[0] - radius, point[1] - radius, point[2] - radius];
        let qmax = [radius + point[0], radius + point[1], radius + point[2]];
        // (centre, half, node, level)
        let mut stack: Vec<([f32; 3], [f32; 3], i32, i32)> =
            vec![([0.0; 3], [WORLD * 0.5, WORLD * 0.5, 0.0], 0, 0)];
        while let Some(&(c, h, node, level)) = stack.last() {
            if self.counts[node as usize] <= 0 {
                stack.pop();
                continue;
            }
            // The loose box: the square doubled, the iterator's z from −1 to 1.
            let e = [2.0 * h[0], 2.0 * h[1], 1.0 * h[2]];
            let bmin = [c[0] - e[0] + 0.0, c[1] - e[1] + 0.0, c[2] - e[2] + -1.0];
            let bmax = [e[0] + c[0] + 0.0, e[1] + c[1] + 0.0, e[2] + c[2] + 1.0];
            let meets = (0..3).all(|k| bmin[k] <= qmax[k] && qmin[k] <= bmax[k]);
            if !meets {
                stack.pop();
                continue;
            }
            let head = self.heads[node as usize];
            if 0 <= head {
                let mut s = head;
                loop {
                    let slot = &self.slots[s as usize];
                    each(slot);
                    s = slot.next;
                    if s == head {
                        break;
                    }
                }
            }
            // FUN_140019290: the node gives way to its children.
            stack.pop();
            if level + 1 < LEVELS {
                let (hx, hy, hz) = (h[0] * 0.5, h[1] * 0.5, h[2]);
                let child = |dx: f32, dy: f32| [c[0] + dx, c[1] + dy, c[2] + hz];
                let half = [hx, hy, hz];
                stack.push((child(hx * -1.0, hy * -1.0), half, node * 4 + 1, level + 1));
                stack.push((child(hx, hy * -1.0), half, node * 4 + 2, level + 1));
                stack.push((child(hx * -1.0, hy), half, node * 4 + 3, level + 1));
                stack.push((child(hx * 1.0, hy * 1.0), half, node * 4 + 4, level + 1));
            }
        }
    }

    /// The characters standing within `radius` of `point` (squared distance below `radius²`), in
    /// the tree's order.
    ///
    /// Original: `FUN_140097e60`.
    pub fn within(&self, point: [f32; 3], radius: f32, filter: impl Fn(Who) -> bool) -> Vec<Who> {
        let mut out = Vec::new();
        let r2 = radius * radius;
        self.visit(point, radius, |who, p| {
            if filter(who) && dist2(p, point) < r2 {
                out.push(who);
            }
        });
        out
    }

    /// The nearest character within `radius` of `point` that `filter` takes; the first of equals in
    /// the tree's order.
    ///
    /// Original: `FUN_140097730`.
    pub fn nearest(
        &self,
        point: [f32; 3],
        radius: f32,
        filter: impl Fn(Who) -> bool,
    ) -> Option<Who> {
        let mut best = None;
        let mut most = radius * radius;
        self.visit(point, radius, |who, p| {
            if filter(who) {
                let d = dist2(p, point);
                if d < most {
                    most = d;
                    best = Some(who);
                }
            }
        });
        best
    }
}

/// `Vector3::Dot` of the difference with itself.
fn dist2(p: [f32; 3], q: [f32; 3]) -> f32 {
    let (dx, dy, dz) = (p[0] - q[0], p[1] - q[1], p[2] - q[2]);
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_lands_in_the_deepest_level_cell() {
        // Radius 0.25: level 7, 128 cells of 16 a side.
        assert_eq!(
            node_of([0.0, 0.0, 0.0], 0.25),
            0x1555 + morton(64, 64) as i32
        );
        assert_eq!(node_of([-1024.0, -1024.0, 0.0], 0.25), 0x1555);
        assert_eq!(
            node_of([17.0, -3.0, 5.0], 0.25),
            0x1555 + morton(65, 63) as i32
        );
    }

    #[test]
    fn slots_are_reused_last_freed_first_and_lists_keep_arrival_order() {
        let mut t = CharacterTree::default();
        let a = t.insert(Who::Citizen(0), [1.0, 1.0, 0.0], 0.25);
        let b = t.insert(Who::Citizen(1), [2.0, 2.0, 0.0], 0.25);
        let c = t.insert(Who::Citizen(2), [3.0, 3.0, 0.0], 0.25);
        assert_eq!((a, b, c), (0, 1, 2));
        t.remove(a);
        t.remove(c);
        assert_eq!(t.insert(Who::Animal(9), [1.0, 1.0, 0.0], 0.25), 2);
        let found = t.within([2.0, 2.0, 0.0], 8.0, |_| true);
        assert_eq!(found, vec![Who::Citizen(1), Who::Animal(9)]);
        // Moving away and back puts it at the end of its node.
        t.place(b, [200.0, 200.0, 0.0], 0.25);
        t.place(b, [2.0, 2.0, 0.0], 0.25);
        assert_eq!(
            t.within([2.0, 2.0, 0.0], 8.0, |_| true),
            vec![Who::Animal(9), Who::Citizen(1)]
        );
    }

    #[test]
    fn a_later_character_ahead_going_the_same_way_is_followed() {
        let mut t = CharacterTree::default();
        let a = t.insert(Who::Citizen(0), [10.0, 10.0, 0.0], 0.25);
        let b = t.insert(Who::Citizen(1), [10.3, 10.0, 0.0], 0.25);
        t.set_direction(a, [1.0, 0.0, 0.0]);
        t.set_direction(b, [1.0, 0.0, 0.0]);
        assert!(t.follows(a));
        // The later one does not follow the earlier.
        assert!(!t.follows(b));
        // Opposite ways: not followed.
        t.set_direction(b, [-1.0, 0.0, 0.0]);
        assert!(!t.follows(a));
    }

    #[test]
    fn nothing_is_found_far_from_height_zero() {
        let mut t = CharacterTree::default();
        t.insert(Who::Animal(1), [5.0, 5.0, 12.0], 0.25);
        t.insert(Who::Animal(2), [5.0, 6.0, 8.0], 0.25);
        // Point at 12 with radius 8: the query's lowest z (4) is above the nodes' (1).
        assert_eq!(t.nearest([5.0, 5.5, 12.0], 8.0, |_| true), None);
        // At 9 it reaches down to 1.
        assert_eq!(
            t.nearest([5.0, 5.5, 9.0], 8.0, |_| true),
            Some(Who::Animal(2))
        );
    }
}

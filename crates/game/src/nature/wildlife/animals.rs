//! Animals: creating and placing them, their schedule, one update (wander, idle, grow up, die), and being hunted.

use super::*;

impl Wild<'_, '_> {
    /// `FUN_1400c8bf0` + `FUN_1400c9d70` for an entity without a map footprint: a placed
    /// entry registered in its template's bucket.
    pub(super) fn create(&mut self, template: usize, position: [f32; 3]) -> usize {
        let id = self.place(template, position);
        let handle = self.scheduler.register(id, self.templates[template].bucket);
        debug_assert_eq!(handle, id);
        id
    }

    /// The placed entry of [`Wild::create`] alone (its owner registers it in the buckets).
    pub(super) fn place(&mut self, template: usize, position: [f32; 3]) -> usize {
        let id = self.placed.len();
        let cell = self.cell_of(position);
        let n = self.n();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell: (cell % n, cell / n),
            footprint: (1, 1),
            position,
            yaw: 0.0,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        id
    }

    /// `FUN_1400c8bf0`'s ModelComponent: the variant (random when the model says so) and its
    /// mesh.
    pub(super) fn pick_variant(&mut self, template: usize) -> Option<(usize, usize)> {
        self.wildlife.animal_kinds.get(&template)?;
        let t = &self.templates[template];
        let (mut variant, mut mesh) = (0usize, 0usize);
        if let Some(m) = &t.model {
            variant = m.variant.max(0) as usize;
            if m.random_variant {
                variant = self.rng.rand_below(m.variants.len() as u32) as usize;
            }
            mesh = m.mesh.max(0) as usize;
        }
        Some((variant, mesh))
    }

    /// A new animal of `template` with the variant [`Wild::pick_variant`] chose
    /// (`FUN_14010c8d0`: placed, turned, wild; a male takes the variant's second mesh).
    pub(super) fn insert_animal(
        &mut self,
        template: usize,
        (variant, mut mesh): (usize, usize),
        position: [f32; 3],
        yaw: f32,
        male: bool,
    ) -> Option<usize> {
        let kind = self.wildlife.animal_kinds.get(&template)?.clone();
        if let Some(m) = &self.templates[template].model {
            if male && m.variants.get(variant).is_some_and(|v| v.meshes.len() > 1) {
                mesh = 1;
            }
        }
        let id = self.placed.len();
        let mut character = Character::new(kind.character, position, 1.0);
        character.yaw = yaw;
        self.wildlife.animals.insert(
            id,
            Animal {
                template,
                character,
                herd: None,
                flags: WILD,
                home: -1,
                timer: 0,
                area: [0; 4],
                male,
                age: 0.0,
                life: 0.0,
                breed: 0,
                livestock: self
                    .wildlife
                    .livestock_kinds
                    .contains_key(&template)
                    .then(Livestock::default),
                variant,
                mesh,
                scale: 1.0,
                dying: false,
            },
        );
        self.rescale(id);
        Some(id)
    }

    /// `FUN_1400c9d70` for an animal: its entity, the setup (`FUN_14010c220`, the lifespan),
    /// the activation (`FUN_14010c0f0`: home where it stands, its area around it, then the
    /// scheduled update with no time).
    pub(super) fn register_animal(&mut self, a: usize) {
        self.register_animal_in(a, true);
    }

    /// [`Wild::register_animal`]; without `buckets` the owner registers the entity.
    pub(super) fn register_animal_in(&mut self, a: usize, buckets: bool) {
        let template = self.wildlife.animals[&a].template;
        let position = self.wildlife.animals[&a].character.position;
        let e = if buckets {
            self.create(template, position)
        } else {
            self.place(template, position)
        };
        debug_assert_eq!(e, a);
        self.wildlife.kinds.insert(a, Kind::Animal);
        // CharacterComponent's start (`FUN_14010ee90`): into the tree.
        let ch = &self.wildlife.animals[&a].character;
        let slot = self.wildlife.tree.insert(
            crate::behaviour::character_tree::Who::Animal(a),
            ch.position,
            ch.desc.radius,
        );
        self.wildlife
            .animals
            .get_mut(&a)
            .unwrap()
            .character
            .tree_slot = slot;
        let d = self.wildlife.animal_kinds[&template].desc.clone();
        let life = self.rng.rand_float_tol() * d.life_range + d.life;
        let n = self.n();
        let cell = self.cell_of(position);
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        animal.life = life;
        if animal.flags & WILD != 0 {
            animal.home = cell;
            animal.area = area_around(n, cell, d.area);
            self.update_animal(a, 0);
        }
        // LivestockComponent's activation (`FUN_140129140`): without a pasture it waits in the
        // town's list.
        let animal = &self.wildlife.animals[&a];
        if animal.livestock.is_some_and(|l| l.pasture.is_none()) {
            self.wildlife.unassigned.push(a);
        }
    }

    /// `FUN_14010c7f0`: the model grows from a third to full size by the adult age.
    pub(super) fn rescale(&mut self, a: usize) {
        let Some(animal) = self.wildlife.animals.get_mut(&a) else {
            return;
        };
        let d = &self.wildlife.animal_kinds[&animal.template].desc;
        let s = (animal.age / d.adult * 0.67 + 0.33).min(1.0);
        animal.scale = s;
        animal.character.scale = s;
    }

    /// A scheduled visit of entity `e` with `elapsed` ticks.
    pub fn scheduled(&mut self, e: usize, elapsed: i64) {
        match self.wildlife.kinds.get(&e).copied() {
            Some(Kind::Population(p)) => self.update_population(p, elapsed),
            Some(Kind::Herd(h)) => self.update_herd(h, elapsed),
            Some(Kind::Animal) => {
                // CharacterComponent's scheduled notify (`FUN_14010e950`): MoveTo's follow
                // check (`FUN_1400cde40`).
                if let Some(animal) = self.wildlife.animals.get_mut(&e) {
                    let following = self.wildlife.tree.follows(animal.character.tree_slot);
                    animal.character.stack.notify_moves(following);
                }
                self.update_animal(e, elapsed);
                self.update_livestock(e, elapsed);
            }
            None => {}
        }
    }

    /// AnimalComponent's scheduled update.
    ///
    /// Original: `FUN_14010b8f0`.
    pub fn update_animal(&mut self, a: usize, dt: i64) {
        let Some(animal) = self.wildlife.animals.get_mut(&a) else {
            return;
        };
        if animal.dying {
            return;
        }
        let d = self.wildlife.animal_kinds[&animal.template].desc.clone();
        animal.age += seconds(dt) * (1.0 / 3600.0);
        if animal.life < animal.age {
            self.kill(a);
            return;
        }
        // A breeding male: once grown, young every `breed` years and up to as much again.
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        if animal.male && animal.flags & BIRTH == 0 && animal.flags & BREEDER != 0 {
            if animal.breed != 0 || animal.age <= d.adult {
                if 0 < animal.breed {
                    animal.breed -= dt;
                    if animal.breed <= 0 {
                        // `FUN_14011de20(.., 0x200000, 0)`.
                        animal.flags |= BIRTH;
                        animal.breed = 0;
                    }
                }
            } else {
                let years = self.rng.rand_float() * d.breed + d.breed;
                self.wildlife.animals.get_mut(&a).unwrap().breed = ticks(years * 3600.0);
            }
        }
        self.rescale(a);
        let n = self.n();
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        if animal.character.stack.is_active() {
            return;
        }
        animal.timer -= dt;
        if 0 < animal.timer {
            return;
        }
        let mut anim = 1;
        if animal.flags & HUNTED == 0 {
            if animal.flags & WILD != 0 {
                let home = animal.home;
                // A home under an occupant other than a trading post (`+0x00`, its
                // TradeComponent) or no longer ground: a new home and area.
                let taken = home >= 0
                    && self.cells.occupants[home as usize]
                        .is_some_and(|o| !self.templates[self.placed[o].template].trading_post);
                if home < 0 || self.cells.types[home as usize] != CellType(2) || taken {
                    self.rehome(a, &d);
                }
            }
            let animal = &self.wildlife.animals[&a];
            let here = self.cell_of(animal.character.position);
            let area = animal.area;
            let mut wander = !inside(&area, here, n);
            if !wander {
                anim = self.rng.rand_below(4) as usize;
                wander = anim == 0;
            }
            if wander {
                let mut in_area = 0;
                let mut total = 0;
                let mut dest;
                loop {
                    let area = self.wildlife.animals[&a].area;
                    let ry = self.rng.rand_below(area[3] as u32) as i32;
                    let rx = self.rng.rand_below(area[2] as u32) as i32;
                    let cell = rx + area[0] as i32 + (ry + area[1] as i32) * n;
                    let base = self.ground(cell);
                    let first = self.rng.rand_float_tol();
                    let second = self.rng.rand_float_tol();
                    dest = [base[0] + second * 0.4, base[1] + first * 0.4, base[2]];
                    if 10 < in_area {
                        self.rehome(a, &d);
                        in_area = 0;
                    }
                    in_area += 1;
                    total += 1;
                    let at = self.cell_of(dest);
                    if self.cells.types[at as usize] == CellType(2) || 100 <= total {
                        break;
                    }
                }
                if self.cells.types[self.cell_of(dest) as usize] == CellType(2) {
                    let walk = MoveTo::new(dest, 0, 1, ANIMAL_WALK, 1, -1.0, self.rng);
                    self.restack(a, Some(Action::move_to(walk)));
                    self.wildlife.animals.get_mut(&a).unwrap().timer = 0;
                    return;
                }
                anim = self.rng.rand_below(3) as usize + 1;
            }
        }
        let idle = self.rng.rand_float() * d.idle + d.idle;
        self.idle(a, anim, idle, d.turn);
    }

    /// `FUN_1400ca8d0` on the animal's stack, then (with `action`) `FUN_1400ca9b0`.
    pub(super) fn restack(&mut self, a: usize, action: Option<Action>) {
        let Some(animal) = self.wildlife.animals.get(&a) else {
            return;
        };
        let Some(group) = self
            .wildlife
            .groups
            .get(&(animal.template, animal.variant))
            .cloned()
        else {
            return;
        };
        let ch = &mut self.wildlife.animals.get_mut(&a).unwrap().character;
        let mut stack = std::mem::take(&mut ch.stack);
        stack.clear(ch, &group, &mut []);
        if let Some(action) = action {
            stack.push(ch, &group, action);
        }
        ch.stack = stack;
    }

    /// A hunter has the animal: it stops what it does and stands.
    ///
    /// Original: `FUN_14010c730`.
    pub fn hunted(&mut self, a: usize) {
        let Some(animal) = self.wildlife.animals.get_mut(&a) else {
            return;
        };
        animal.flags |= HUNTED;
        if animal.character.stack.is_active() {
            self.restack(a, None);
            self.update_animal(a, 0);
        }
    }

    /// `FUN_14010b720`: play clip `anim` for `seconds`; with `turn`, face a random way.
    pub(super) fn idle(&mut self, a: usize, anim: usize, secs: f32, turn: bool) {
        let group = {
            let animal = &self.wildlife.animals[&a];
            self.wildlife
                .groups
                .get(&(animal.template, animal.variant))
                .cloned()
        };
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        match &group {
            Some(g) => animal.character.animation.play(g, anim, 0),
            None => animal.character.animation = AnimationStack::new(anim, 0),
        }
        animal.timer = ticks(secs);
        if turn {
            // FUN_14010b720: (second draw, first draw, 0) normalized; the direction that
            // normalized again, the heading of the first (`Vector3::Heading`).
            let y = self.rng.rand_float_tol();
            let x = self.rng.rand_float_tol();
            let v = crate::math::normalize3([x, y, 0.0]);
            let ch = &mut self.wildlife.animals.get_mut(&a).unwrap().character;
            ch.flag_28 = false;
            ch.direction = crate::math::normalize3(v);
            ch.target_heading = crate::math::heading(v[0], v[1]);
        }
    }

    /// `FUN_14010b4f0` then `FUN_14010b2f0`: the nearest free ground in growing squares around
    /// the home becomes the home, and the area follows.
    pub(super) fn rehome(&mut self, a: usize, d: &AnimalDescription) {
        let n = self.n();
        let home = self.wildlife.animals[&a].home.max(0);
        let new = nearest_ground(self.cells, n, home);
        let animal = self.wildlife.animals.get_mut(&a).unwrap();
        animal.home = new;
        animal.area = area_around(n, new, d.area);
    }
}

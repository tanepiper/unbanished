//! Starting households: every profile row is a couple plus up to `extra` children, created on the
//! game's shared random stream. See docs/CITIZEN-NEEDS-AND-HOUSEHOLDS.md and
//! START-CONDITION-PROFILES.md.
//!
//! Original: `FUN_1400a62a0`, `FUN_1400a6510`, `FUN_14011c550`.

use banished_formats::names::NameDatabase;

use crate::people::names::citizen_name;
use crate::random::Random;

/// The `CitizenDescription` values household creation reads.
#[derive(Debug, Clone, Copy)]
pub struct CitizenRules {
    // +0x44
    /// Not identified yet.
    pub adult_age: f32,
    // +0x50, +0x54
    /// Starting adults' ages are `[min, max / 2)`.
    pub start_age: (f32, f32),
    // +0x38, +0x3c
    /// Lifespan base and range.
    pub lifespan: (f32, f32),
    // +0x118
    /// Education granted to educated starting adults.
    pub education: i32,
}

/// A citizen: name, sex, age, education, appearance and family.
#[derive(Debug, Clone, PartialEq)]
pub struct Citizen {
    // +0x28
    /// Not identified yet.
    pub name: String,
    // +0x38
    /// 1 = male.
    pub male: bool,
    // +0x3c
    /// The citizen's age, in the description's age units.
    pub age: f32,
    // +0x40
    /// Not identified yet.
    pub lifespan: f32,
    // +0x34
    /// Appearance variation `(Rand + Rand) & 7`, passed to the shader as instance `w`.
    pub appearance: u32,
    // +0x220; +0x210
    /// Whether the citizen is educated.
    pub educated: bool,
    /// The education.
    pub education: i32,
    /// Map position (cells from the terrain centre) and height.
    pub position: [f32; 3],
    // +0x250
    /// Spouse (index into the citizen list).
    pub spouse: Option<usize>,
    /// Household parent the child was added to.
    ///
    /// Original: `FUN_14011bd70`.
    pub parent: Option<usize>,
    // +0x208
    /// School (none at New Game).
    pub school: Option<usize>,
    /// False once the entity is deleted (the port keeps its index).
    pub alive: bool,
}

impl Citizen {
    /// Whether the citizen is an adult.
    pub fn is_adult(&self, rules: &CitizenRules) -> bool {
        self.age >= rules.adult_age
    }
}

/// The starting households of a new town, as the start condition makes them.
pub struct Households<'a> {
    /// The citizen rules.
    pub rules: CitizenRules,
    /// The men's names.
    pub male_names: &'a NameDatabase,
    /// The women's names.
    pub female_names: &'a NameDatabase,
    // MapGenerator +0x40
    /// `MapGenerator`: spread of citizens around the start point.
    pub spread: f32,
    /// Start cell centre on the ground.
    ///
    /// Original: `FUN_140002080`.
    pub start: [f32; 3],
    /// The citizens made.
    pub citizens: Vec<Citizen>,
}

/// `DAT_140178814`.
const HALF: f32 = 0.5;

impl Households<'_> {
    /// `FUN_1400a6510` + `FUN_14011c550`: create and register one citizen.
    fn create(&mut self, rng: &mut Random, age: f32, female: bool, educated: bool) -> usize {
        // Entity construction: no component setup draws for the citizen template (its model has
        // a fixed variant).
        let child = age < self.rules.adult_age;
        let educated = educated && !child;
        let dy = rng.rand_float_tol();
        let dx = rng.rand_float_tol();
        let position = [
            dx * self.spread + self.start[0],
            dy * self.spread + self.start[1],
            self.start[2] + 0.0,
        ];
        let male = !female;
        let db = if male {
            self.male_names
        } else {
            self.female_names
        };
        let name = citizen_name(db, rng);
        let a = rng.rand();
        let b = rng.rand();
        let appearance = a.wrapping_add(b) & 7;
        let lifespan = rng.rand_float_tol() * self.rules.lifespan.1 + self.rules.lifespan.0;
        self.citizens.push(Citizen {
            name,
            male,
            age,
            lifespan,
            appearance,
            educated,
            education: if educated { self.rules.education } else { 0 },
            position,
            spouse: None,
            parent: None,
            school: None,
            alive: true,
        });
        self.citizens.len() - 1
    }

    /// One couple per row, then `Rand(extra + 1)` children whose sex alternates across all
    /// households, starting female.
    ///
    /// Original: `FUN_1400a62a0`.
    pub fn create_all(&mut self, rng: &mut Random, rows: &[(bool, i32)]) {
        let (lo, hi) = self.rules.start_age;
        let mut female_child = true;
        for &(educated, extra) in rows {
            let age = rng.rand_float() * (hi * HALF - lo) + lo;
            let husband = self.create(rng, age, false, educated);
            let age = rng.rand_float() * (hi * HALF - lo) + lo;
            let wife = self.create(rng, age, true, educated);
            self.citizens[husband].spouse = Some(wife);
            self.citizens[wife].spouse = Some(husband);
            let children = rng.rand_below((extra + 1) as u32);
            for _ in 0..children {
                let age = rng.rand_float() * self.rules.adult_age;
                let child = self.create(rng, age, female_child, false);
                female_child = !female_child;
                self.citizens[child].parent = Some(wife);
            }
        }
    }
}

/// A child born to `mother`: its sex, a name from that sex's database, the
/// appearance, no education and a lifespan. Position, age and family are set by the caller.
///
/// Original: `FUN_14011c550`, called from `FUN_140117060`.
pub fn newborn(
    rng: &mut Random,
    rules: &CitizenRules,
    male_names: &NameDatabase,
    female_names: &NameDatabase,
    mother: &Citizen,
    male: bool,
) -> Citizen {
    let name = citizen_name(if male { male_names } else { female_names }, rng);
    let a = rng.rand();
    let b = rng.rand();
    let appearance = a.wrapping_add(b) & 7;
    let lifespan = rng.rand_float_tol() * rules.lifespan.1 + rules.lifespan.0;
    Citizen {
        name,
        male,
        age: 0.0,
        lifespan,
        appearance,
        educated: false,
        education: 0,
        position: mother.position,
        spouse: None,
        parent: None,
        school: None,
        alive: true,
    }
}

/// Status Bar population: adults, students, children.
///
/// Original: `FUN_14007eda0`.
pub fn population(citizens: &[Citizen], rules: &CitizenRules) -> (u32, u32, u32) {
    let (mut adults, mut students, mut children) = (0, 0, 0);
    for c in citizens.iter().filter(|c| c.alive) {
        if c.age < rules.adult_age || c.school.is_some() {
            if c.school.is_none() {
                children += 1;
            } else {
                students += 1;
            }
        } else {
            adults += 1;
        }
    }
    (adults, students, children)
}

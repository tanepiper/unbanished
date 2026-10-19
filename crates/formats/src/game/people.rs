//! Citizens and their lives: citizens, diseases, happiness, population, immigration, nomads, cemeteries and tombstones.

use super::*;

/// `Disease` (version 2, `Game/Disease/Diseases.rsc` lists them).
///
/// Original: `FUN_1400987a0`.
#[derive(Debug, Clone, PartialEq)]
pub struct Disease {
    // +0x20; +0x28
    /// The string table of the name.
    pub table: Option<String>,
    /// The name's key.
    pub name: String,
    // +0x30
    /// Spread interval in 300 s units.
    ///
    /// Original: `FUN_140117ca0`.
    pub spread_interval: f32,
    // +0x34
    /// Chance a spread attempt infects.
    ///
    /// Original: `FUN_140117940`.
    pub spread_chance: f32,
    // +0x38
    /// Spread radius.
    pub spread_radius: f32,
    // +0x3c
    /// Weight when an outbreak picks a disease.
    ///
    /// Original: `FUN_140117af0`.
    pub weight: f32,
    // +0x40
    /// How fast a sick citizen recovers: a citizen recovers when a random float is greater than
    /// `1 / recovery / doctor`.
    ///
    /// Original: used by `FUN_140117880`.
    pub recovery: f32,
}

/// Read a [`Disease`] from the bytes of its resource.
pub fn disease(data: &[u8]) -> Result<Disease> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("Disease") {
            return format_err(format!("expected Disease, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            Ok(Disease {
                table: r.external_ref()?,
                name: r.string()?,
                spread_interval: r.f32()?,
                spread_chance: r.f32()?,
                spread_radius: r.f32()?,
                weight: r.f32()?,
                recovery: r.f32()?,
            })
        })
    })?;
    Ok(value)
}

// +0x20
/// `AttendanceDescription`: the capacity (Chapel 200, DoctorHouse 30, SchoolHouse 20).
///
/// Original: `FUN_140045010`.
pub fn attendance_description(data: &[u8]) -> Result<i32> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("AttendanceDescription") {
            return format_err(format!(
                "expected AttendanceDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| r.i32())
    })?;
    Ok(value)
}

/// `HappinessDescription`.
///
/// Original: `FUN_14004b610`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HappinessDescription {
    // +0x20
    /// The kind, which picks the map layer (0 well, 1 market and trading post, 2 tavern, 3 chapel
    /// and cemetery, 4 doctor, 5 the unhappy mines and quarry).
    pub kind: i32,
    // +0x24
    /// Only while someone works there.
    pub needs_workers: bool,
    // +0x25
    /// Only while the storage holds goods matching `mask`, one consumed per visit.
    pub needs_stock: bool,
    // +0x28
    /// Raw-material flags.
    pub mask: u32,
    // +0x2c
    /// Cells around the building a loitering citizen stands in.
    pub pad: i32,
}

/// Read a [`HappinessDescription`] from the bytes of its resource.
pub fn happiness_description(data: &[u8]) -> Result<HappinessDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("HappinessDescription") {
            return format_err(format!(
                "expected HappinessDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(HappinessDescription {
                kind: r.i32()?,
                needs_workers: r.bool()?,
                needs_stock: r.bool()?,
                mask: r.u32()?,
                pad: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

/// `PopulationDescription`: the wildlife manager's grid and the herds it starts.
///
/// Original: `FUN_140055140`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopulationDescription {
    // +0x20
    /// The display name.
    pub name: String,
    // +0x28
    /// Map cells per population cell (a side).
    pub cell: i32,
    // +0x2c
    /// Map cells per herd block (a side).
    pub herd_block: i32,
    // +0x30
    /// Cell types that count as habitat.
    pub habitat: u32,
    // +0x34
    /// Cell types that rule a population cell out.
    pub forbidden: u32,
    // +0x38
    /// Map occupants rule a population cell out.
    pub no_occupants: bool,
    // +0x3c
    /// A habitable cell's capacity.
    pub capacity: i32,
    // +0x40
    /// Regrowth per 300 s.
    pub regrowth: i32,
    // +0x44
    /// Not identified yet.
    pub value_44: i32,
    // +0x48
    /// Recount a sixteenth of the cells every update.
    pub refresh: bool,
    // +0x50
    /// The herd entity template.
    pub herd: Option<String>,
}

/// Read a [`PopulationDescription`] from the bytes of its resource.
pub fn population_description(data: &[u8]) -> Result<PopulationDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("PopulationDescription") {
            return format_err(format!(
                "expected PopulationDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let cell = r.i32()?;
            let herd_block = r.i32()?;
            let habitat = r.u32()?;
            let forbidden = r.u32()?;
            let no_occupants = r.bool()?;
            let capacity = r.i32()?;
            let regrowth = r.i32()?;
            let value_44 = r.i32()?;
            let refresh = r.bool()?;
            let herd = r.external_ref()?;
            let name = r.string()?;
            Ok(PopulationDescription {
                name,
                cell,
                herd_block,
                habitat,
                forbidden,
                no_occupants,
                capacity,
                regrowth,
                value_44,
                refresh,
                herd,
            })
        })
    })?;
    Ok(value)
}

/// A `DiseaseDescription`: blight on fields, orchards and pastures. Times are in months of 300
/// seconds.
///
/// Original: `FUN_1400497a0`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DiseaseDescription {
    // +0x20..=+0x24
    /// The months in which it can start, both included.
    pub months: (i32, i32),
    // +0x28
    /// The odds of a start: `1 / (12 / check × odds)` at each check.
    pub odds: f32,
    // +0x2c
    /// The odds instead of `odds` when the cell grew the same crop last time.
    pub odds_same: f32,
    // +0x30
    /// The time between checks.
    pub check: f32,
    // +0x34
    /// The time between kills of a crop, a tree or an animal.
    pub kill: f32,
    // +0x38
    /// The time between spreads.
    pub spread: f32,
    // +0x3c
    /// How long the blight lasts.
    pub duration: f32,
    // +0x40
    /// How far it spreads, in cells.
    pub radius: i32,
}

/// Read a [`DiseaseDescription`] from the bytes of its resource.
pub fn disease_description(data: &[u8]) -> Result<DiseaseDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("DiseaseDescription") {
            return format_err(format!(
                "expected DiseaseDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(DiseaseDescription {
                months: (r.i32()?, r.i32()?),
                odds: r.f32()?,
                odds_same: r.f32()?,
                check: r.f32()?,
                kill: r.f32()?,
                spread: r.f32()?,
                duration: r.f32()?,
                radius: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

// +0x20
/// `CemeteryDescription`: the tombstone template.
///
/// Original: `FUN_140048160`.
pub fn cemetery_description(data: &[u8]) -> Result<Option<String>> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CemeteryDescription") {
            return format_err(format!(
                "expected CemeteryDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| r.external_ref())
    })?;
    Ok(value)
}

/// An `ImmigrationDescription`: when nomads come to the town. Two values are not serialised:
/// the month in which nomads leave (7) and the margin from the map's edge (10).
///
/// Original: `FUN_14004f150`, `FUN_14004f300`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImmigrationDescription {
    // +0x20
    /// The nomad template.
    pub nomad: Option<String>,
    // +0x28
    /// The population the town needs.
    pub population: i32,
    // +0x30
    /// The templates the town must have (for example a town hall).
    pub required: Vec<Option<String>>,
    // +0x40/+0x44
    /// The band's size as a share of the population.
    pub share: (f32, f32),
    // +0x48/+0x4c
    /// The months between bands: base and random range.
    pub wait: (f32, f32),
    // +0x50/+0x54
    /// The months in which a band may come.
    pub months: (i32, i32),
    // +0x60
    /// The risk that visitors bring disease.
    pub risk: f32,
    // +0x64
    /// The months of that risk.
    pub risk_months: i32,
}

/// Read a [`ImmigrationDescription`] from the bytes of its resource.
pub fn immigration_description(data: &[u8]) -> Result<ImmigrationDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ImmigrationDescription") {
            return format_err(format!(
                "expected ImmigrationDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let nomad = r.external_ref()?;
            let population = r.i32()?;
            let required = (0..r.count()?)
                .map(|_| r.external_ref())
                .collect::<Result<_>>()?;
            let share = (r.f32()?, r.f32()?);
            let wait = (r.f32()?, r.f32()?);
            let months = (r.i32()?, r.i32()?);
            let (risk, risk_months) = (r.f32()?, r.i32()?);
            Ok(ImmigrationDescription {
                nomad,
                population,
                required,
                share,
                wait,
                months,
                risk,
                risk_months,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x28; +0x30. FUN_140050990 puts the material in the model's material 0.
/// A `NomadDescription`: the citizen template nomads become, and the men's and women's
/// material instance (put in their model's material 0).
///
/// Original: `FUN_140050880`.
pub fn nomad_description(data: &[u8]) -> Result<(Option<String>, [Option<String>; 2])> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("NomadDescription") {
            return format_err(format!(
                "expected NomadDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let citizen = r.external_ref()?;
            let men = r.external_ref()?;
            let women = r.external_ref()?;
            Ok((citizen, [men, women]))
        })
    })?;
    Ok(value)
}

// (+0x20 + RandFloat × +0x24)
/// `TombstoneDescription`: a grave lasts `(+ RandFloat ×)` × 300 s.
///
/// Original: `FUN_140058380`, `FUN_1400581c0`.
pub fn tombstone_description(data: &[u8]) -> Result<(f32, f32)> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("TombstoneDescription") {
            return format_err(format!(
                "expected TombstoneDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| Ok((r.f32()?, r.f32()?)))
    })?;
    Ok(value)
}

/// `CitizenDescription`. The class has ~60 scalar fields; they are kept keyed by their runtime
/// offset (as the original code reads them) with named getters for the ones whose consumers are
/// traced.
///
/// Original: `FUN_140119dd0`.
#[derive(Debug, Clone)]
pub struct CitizenDescription {
    /// Scalar fields by runtime offset.
    pub fields: std::collections::BTreeMap<u32, Scalar>,
    // +0x138
    /// External references.
    ///
    /// Original: `FUN_14001c620`.
    pub list: Vec<Option<String>>,
    // +0x20; +0x28; +0x30
    /// Not identified yet.
    pub refs: [Option<String>; 3],
}

/// A value of the citizen description: a float or an integer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scalar {
    /// A float.
    F32(f32),
    /// An integer.
    I32(i32),
}

/// Read a [`CitizenDescription`] from the bytes of its resource.
pub fn citizen_description(data: &[u8]) -> Result<CitizenDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CitizenDescription") {
            return format_err(format!(
                "expected CitizenDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            use Scalar::*;
            let mut fields = std::collections::BTreeMap::new();
            let mut f = |r: &mut Reader, o: u32| -> Result<()> {
                fields.insert(o, F32(r.f32()?));
                Ok(())
            };
            for o in [
                0x44, 0x6c, 0x70, 0x74, 0x78, 0x7c, 0x80, 0x84, 0x88, 0x8c, 0x90, 0x38, 0x3c, 0x40,
                0xc0, 0xd8, 0xdc, 0xe0, 0xe4, 0x48, 0x4c, 0x50, 0x54, 0x58, 0x5c, 0x60, 0xc4, 0xd4,
                0xc8, 0xcc, 0xd0, 0xe8, 0xec, 0xf0, 0xf4,
            ] {
                f(r, o)?;
            }
            fields.insert(0x110, I32(r.i32()?));
            let list = (0..r.count()?)
                .map(|_| r.external_ref())
                .collect::<Result<_>>()?;
            fields.insert(0x114, F32(r.f32()?));
            fields.insert(0x118, I32(r.i32()?));
            for o in [0x120, 0x11c, 0x128, 0x124, 0x130, 0xa4] {
                fields.insert(o, F32(r.f32()?));
            }
            fields.insert(0xa8, I32(r.i32()?));
            fields.insert(0xac, I32(r.i32()?));
            fields.insert(0xa0, F32(r.f32()?));
            fields.insert(0xb0, I32(r.i32()?));
            fields.insert(0xb4, F32(r.f32()?));
            fields.insert(0xb8, F32(r.f32()?));
            fields.insert(0xbc, I32(r.i32()?));
            let refs = [r.external_ref()?, r.external_ref()?, r.external_ref()?];
            fields.insert(0x64, F32(r.f32()?));
            fields.insert(0x68, F32(r.f32()?));
            for o in [0xf8, 0xfc, 0x100, 0x104, 0x108] {
                fields.insert(o, I32(r.i32()?));
            }
            fields.insert(0x10c, F32(r.f32()?));
            Ok(CitizenDescription { fields, list, refs })
        })
    })?;
    Ok(value)
}

impl CitizenDescription {
    /// The float at an offset of the description (0 when it is not a float).
    pub fn f(&self, offset: u32) -> f32 {
        match self.fields.get(&offset) {
            Some(Scalar::F32(v)) => *v,
            Some(Scalar::I32(v)) => *v as f32,
            None => 0.0,
        }
    }
    /// The integer at an offset of the description (0 when it is not an integer).
    pub fn i(&self, offset: u32) -> i32 {
        match self.fields.get(&offset) {
            Some(Scalar::I32(v)) => *v,
            Some(Scalar::F32(v)) => *v as i32,
            None => 0,
        }
    }
    // +0x44
    /// Age at which a citizen counts as an adult.
    pub fn adult_age(&self) -> f32 {
        self.f(0x44)
    }
    // +0x50, +0x54; +0x54 / 2
    /// The age range of the starting adults. The game uses half of the second value.
    ///
    /// Original: used by `FUN_1400a62a0`.
    pub fn start_age(&self) -> (f32, f32) {
        (self.f(0x50), self.f(0x54))
    }
    // +0x38, +0x3c
    /// Lifespan base and random range.
    ///
    /// Original: `FUN_14011c550`.
    pub fn lifespan(&self) -> (f32, f32) {
        (self.f(0x38), self.f(0x3c))
    }
}

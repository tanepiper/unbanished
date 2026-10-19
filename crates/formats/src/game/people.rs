//! Citizens and their lives: citizens, diseases, happiness, population, immigration, nomads, cemeteries and tombstones.

use super::*;

/// `Disease` (serializer `FUN_1400987a0`, version 2; `Game/Disease/Diseases.rsc` lists them).
#[derive(Debug, Clone, PartialEq)]
pub struct Disease {
    /// `+0x20` string table and `+0x28` key of the name.
    pub table: Option<String>,
    pub name: String,
    /// `+0x30`: spread interval in 300 s units (`FUN_140117ca0`).
    pub spread_interval: f32,
    /// `+0x34`: chance a spread attempt infects (`FUN_140117940`).
    pub spread_chance: f32,
    /// `+0x38`: spread radius.
    pub spread_radius: f32,
    /// `+0x3c`: weight when an outbreak picks a disease (`FUN_140117af0`).
    pub weight: f32,
    /// `+0x40`: recovery divisor (`FUN_140117880`: recover when `RandFloat > 1 / this / doctor`).
    pub recovery: f32,
}

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

/// `AttendanceDescription` (serializer `FUN_140045010`): the capacity `+0x20` (Chapel 200,
/// DoctorHouse 30, SchoolHouse 20).
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

/// `HappinessDescription` (serializer `FUN_14004b610`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HappinessDescription {
    /// `+0x20`: the kind, which picks the map layer (0 well, 1 market and trading post, 2
    /// tavern, 3 chapel and cemetery, 4 doctor, 5 the unhappy mines and quarry).
    pub kind: i32,
    /// `+0x24`: only while someone works there.
    pub needs_workers: bool,
    /// `+0x25`: only while the storage holds goods matching `mask`, one consumed per visit.
    pub needs_stock: bool,
    /// `+0x28`: raw-material flags.
    pub mask: u32,
    /// `+0x2c`: cells around the building a loitering citizen stands in.
    pub pad: i32,
}

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

/// `PopulationDescription` (serializer `FUN_140055140`): the wildlife manager's grid and the
/// herds it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopulationDescription {
    /// `+0x20`: the display name.
    pub name: String,
    /// `+0x28`: map cells per population cell (a side).
    pub cell: i32,
    /// `+0x2c`: map cells per herd block (a side).
    pub herd_block: i32,
    /// `+0x30`: cell types that count as habitat.
    pub habitat: u32,
    /// `+0x34`: cell types that rule a population cell out.
    pub forbidden: u32,
    /// `+0x38`: map occupants rule a population cell out.
    pub no_occupants: bool,
    /// `+0x3c`: a habitable cell's capacity.
    pub capacity: i32,
    /// `+0x40`: regrowth per 300 s.
    pub regrowth: i32,
    /// `+0x44`.
    pub value_44: i32,
    /// `+0x48`: recount a sixteenth of the cells every update.
    pub refresh: bool,
    /// `+0x50`: the herd entity template.
    pub herd: Option<String>,
}

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

/// `DiseaseDescription` (serializer `FUN_1400497a0`): blight on fields, orchards and pastures.
/// It can start in months `+0x20..=+0x24`, one check every `+0x30` × 300 s, with odds
/// `1 / (12 / check × +0x28)` (`+0x2c` when a cell grew the same thing last), kills a crop,
/// tree or animal every `+0x34` × 300 s, spreads within `+0x40` cells every `+0x38` × 300 s,
/// and lasts `+0x3c` × 300 s.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DiseaseDescription {
    pub months: (i32, i32),
    pub odds: f32,
    pub odds_same: f32,
    pub check: f32,
    pub kill: f32,
    pub spread: f32,
    pub duration: f32,
    pub radius: i32,
}

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

/// `CemeteryDescription` (serializer `FUN_140048160`): the tombstone template `+0x20`.
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

/// `ImmigrationDescription` (serializer `FUN_14004f150`, constructor `FUN_14004f300`): `+0x20`
/// the nomad template, `+0x28` the population needed, `+0x30` the templates the town must have,
/// `+0x40`/`+0x44` the band's share of the population, `+0x48`/`+0x4c` the months between
/// bands (base, random range), `+0x50`/`+0x54` the months a band may come in, `+0x60`/`+0x64`
/// the visitors' disease risk and its months. `+0x58` (7) the leaving month and `+0x5c` (10)
/// the edge margin are not serialized.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImmigrationDescription {
    pub nomad: Option<String>,
    pub population: i32,
    pub required: Vec<Option<String>>,
    pub share: (f32, f32),
    pub wait: (f32, f32),
    pub months: (i32, i32),
    pub risk: f32,
    pub risk_months: i32,
}

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

/// `NomadDescription` (serializer `FUN_140050880`): `+0x20` the citizen template they become,
/// `+0x28` / `+0x30` the men's and women's MaterialInstance (`FUN_140050990` puts it in their
/// model's material 0).
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

/// `TombstoneDescription` (serializer `FUN_140058380`): a grave lasts `(+0x20 + RandFloat ×
/// +0x24)` × 300 s (`FUN_1400581c0`).
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

/// `CitizenDescription` (serializer `FUN_140119dd0`). The class has ~60 scalar fields; they are
/// kept keyed by their runtime offset (as the original code reads them) with named getters for
/// the ones whose consumers are traced.
#[derive(Debug, Clone)]
pub struct CitizenDescription {
    /// Scalar fields by runtime offset.
    pub fields: std::collections::BTreeMap<u32, Scalar>,
    /// `+0x138`: external references (`FUN_14001c620`).
    pub list: Vec<Option<String>>,
    /// `+0x20`, `+0x28`, `+0x30`.
    pub refs: [Option<String>; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scalar {
    F32(f32),
    I32(i32),
}

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
    pub fn f(&self, offset: u32) -> f32 {
        match self.fields.get(&offset) {
            Some(Scalar::F32(v)) => *v,
            Some(Scalar::I32(v)) => *v as f32,
            None => 0.0,
        }
    }
    pub fn i(&self, offset: u32) -> i32 {
        match self.fields.get(&offset) {
            Some(Scalar::I32(v)) => *v,
            Some(Scalar::F32(v)) => *v as i32,
            None => 0,
        }
    }
    /// `+0x44`: age at which a citizen counts as an adult.
    pub fn adult_age(&self) -> f32 {
        self.f(0x44)
    }
    /// `+0x50`, `+0x54`: starting adults' age range (`FUN_1400a62a0` uses `+0x54 / 2`).
    pub fn start_age(&self) -> (f32, f32) {
        (self.f(0x50), self.f(0x54))
    }
    /// `+0x38`, `+0x3c`: lifespan base and random range (`FUN_14011c550`).
    pub fn lifespan(&self) -> (f32, f32) {
        (self.f(0x38), self.f(0x3c))
    }
}

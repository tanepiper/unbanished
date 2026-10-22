//! Achievements, scenarios and tutorials (`Game/Achievement`, `Game/Scenario`). See
//! docs/ACHIEVEMENTS.md and docs/TUTORIAL.md.
//!
//! - `Achievement::Achievement`: its string table, title, description, sprite sheet and sprite, and
//!   its goals.
//! - A string table and text; then each class's own block: BuildingGoal, PopulationGoal, StatGoal,
//!   TimeGoal, FinishTutorialGoal.
//! - `Scenario`: an achievement, then the game it starts.
//! - `Tutorial` and its steps, setups, goals and images.
//!
//! Original: `Achievement::Achievement::Serialize`, `Achievement::Goal`, `FUN_1400dcb70`,
//! `FUN_1400dcd40`, `FUN_1400dfd80`, `FUN_1400dfff0`, `FUN_1400dcea0`, `FUN_1400ae840`,
//! `FUN_1400e3fa0`.
//!
//! Original: Runtime `0x18009eb70`, Runtime `0x18009fd30`.

mod tutorial;
pub use tutorial::{tutorial_part, ImageExtra, Tutorial, TutorialPart, TutorialStep};

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Result};

/// `Achievement::Achievement`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Achievement {
    // +0x20
    /// The string table.
    pub strings: Option<String>,
    // +0x28, +0x30
    /// The title's key.
    pub title: String,
    /// The description's key.
    pub description: String,
    // +0x38; +0x40
    /// The sprite sheet.
    pub sprites: Option<String>,
    /// The sprite.
    pub sprite: String,
    // +0x48
    /// The goals.
    pub goals: Vec<Option<String>>,
}

fn achievement(r: &mut Reader) -> Result<Achievement> {
    r.block(|r, _| {
        let strings = r.external_ref()?;
        let title = r.string()?;
        let description = r.string()?;
        let sprites = r.external_ref()?;
        let sprite = r.string()?;
        let goals = (0..r.count()?)
            .map(|_| r.external_ref())
            .collect::<Result<_>>()?;
        Ok(Achievement {
            strings,
            title,
            description,
            sprites,
            sprite,
            goals,
        })
    })
}

/// A Goal's own fields, by class.
#[derive(Debug, Clone, PartialEq)]
pub enum GoalKind {
    // +0x30; +0x38
    /// Some buildings exist.
    Building {
        /// Whether all of the buildings must exist (else any).
        all: bool,
        /// The buildings (ComponentDescriptions).
        buildings: Vec<String>,
    },
    // +0x30
    /// The population is at least this many citizens.
    Population(i32),
    // +0x30..
    /// A statistic compared with an amount or with a second statistic.
    Stat {
        /// The statistic.
        e30: i32,
        /// The second statistic, for odd comparisons.
        e34: i32,
        /// The comparison: 0/1 equal, 2/3 not, 4/5 less, 6/7 at most, 8/9 more, 10/11 at least.
        e38: i32,
        /// The amount the statistic is compared with.
        amount: i32,
        /// Whether the goal is met until the comparison holds, then fails.
        flag: bool,
        /// The statistic's reference.
        r48: String,
        /// The second statistic's reference.
        r60: String,
        /// Raw-material flags, for the statistics that take them.
        e78: i32,
        /// A number, for the statistics that take it.
        i7c: i32,
    },
    // +0x30; +0x34
    /// A number of years passed.
    Time {
        /// The years.
        years: i32,
        /// Whether the goal must be met within that time (else after it).
        within: bool,
    },
    // +0x30
    /// The tutorial is done.
    FinishTutorial(Option<String>),
}

/// `Achievement::Goal`.
#[derive(Debug, Clone, PartialEq)]
pub struct Goal {
    /// The string table.
    pub strings: Option<String>,
    /// The text's key.
    pub text: String,
    /// The class's own fields.
    pub kind: GoalKind,
}

/// Read a [`Goal`] from the bytes of its resource.
pub fn goal(data: &[u8]) -> Result<Goal> {
    let (_, v) = crs::parse(data, |r, h| {
        let (strings, text) = r.block(|r, _| Ok((r.external_ref()?, r.string()?)))?;
        let kind = r.block(|r, _| {
            Ok(match h.type_id {
                t if t == type_id("BuildingGoal") => GoalKind::Building {
                    all: r.bool()?,
                    buildings: (0..r.count()?).map(|_| r.string()).collect::<Result<_>>()?,
                },
                t if t == type_id("PopulationGoal") => GoalKind::Population(r.i32()?),
                t if t == type_id("StatGoal") => GoalKind::Stat {
                    e30: r.i32()?,
                    e34: r.i32()?,
                    e38: r.i32()?,
                    amount: r.i32()?,
                    flag: r.bool()?,
                    r48: r.string()?,
                    r60: r.string()?,
                    e78: r.i32()?,
                    i7c: r.i32()?,
                },
                t if t == type_id("TimeGoal") => GoalKind::Time {
                    years: r.i32()?,
                    within: r.bool()?,
                },
                t if t == type_id("FinishTutorialGoal") => {
                    GoalKind::FinishTutorial(r.external_ref()?)
                }
                t => return format_err(format!("unknown goal type {t:#010x}")),
            })
        })?;
        Ok(Goal {
            strings,
            text,
            kind,
        })
    })?;
    Ok(v)
}

/// Read a [`Achievement`] from the bytes of its resource.
pub fn achievement_description(data: &[u8]) -> Result<Achievement> {
    let (_, v) = crs::parse(data, |r, h| {
        if h.type_id != type_id("Achievement") {
            return format_err(format!("expected Achievement, found {:#010x}", h.type_id));
        }
        achievement(r)
    })?;
    Ok(v)
}

/// `Scenario` (an Achievement).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scenario {
    /// The achievement.
    pub achievement: Achievement,
    // +0x60; +0x64
    /// Not identified yet.
    pub i60: i32,
    /// Not identified yet.
    pub i64: i32,
    // +0x58
    /// The town's name.
    pub town_name: String,
    // +0x70; +0x78; +0x80
    /// Not identified yet.
    pub r70: Option<String>,
    /// Not identified yet.
    pub r78: Option<String>,
    /// Not identified yet.
    pub r80: Option<String>,
    // +0x88
    /// The save to start from (`Save\{name}.sav` in the data, the tutorials'), empty for a new
    /// game.
    ///
    /// Original: `FUN_14007d180`.
    pub save: String,
    // +0x68; +0x69
    /// Not identified yet.
    pub b68: bool,
    /// Not identified yet.
    pub b69: bool,
}

/// Read a [`Scenario`] from the bytes of its resource.
pub fn scenario(data: &[u8]) -> Result<Scenario> {
    let (_, v) = crs::parse(data, |r, h| {
        if h.type_id != type_id("Scenario") {
            return format_err(format!("expected Scenario, found {:#010x}", h.type_id));
        }
        let achievement = achievement(r)?;
        r.block(|r, _| {
            Ok(Scenario {
                achievement,
                i60: r.i32()?,
                i64: r.i32()?,
                town_name: r.string()?,
                r70: r.external_ref()?,
                r78: r.external_ref()?,
                r80: r.external_ref()?,
                save: r.string()?,
                b68: r.bool()?,
                b69: r.bool()?,
            })
        })
    })?;
    Ok(v)
}

fn refs(r: &mut Reader) -> Result<Vec<Option<String>>> {
    (0..r.count()?).map(|_| r.external_ref()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shipped achievement, goal, scenario and tutorial part parses whole.
    #[test]
    fn every_achievement_and_tutorial_parses() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-data/extracted");
        let mut counts = std::collections::BTreeMap::<&str, usize>::new();
        for dir in ["Game/Achievement", "Game/Scenario"] {
            let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
                return;
            };
            for e in entries {
                let p = e.unwrap().path();
                let b = std::fs::read(&p).unwrap();
                let Ok(h) = crs::header(&b) else { continue };
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                let t = h.type_id;
                let kind = if t == type_id("Achievement") {
                    achievement_description(&b).map(|_| "achievement")
                } else if t == type_id("Scenario") {
                    scenario(&b).map(|_| "scenario")
                } else if [
                    "BuildingGoal",
                    "PopulationGoal",
                    "StatGoal",
                    "TimeGoal",
                    "FinishTutorialGoal",
                ]
                .iter()
                .any(|n| t == type_id(n))
                {
                    goal(&b).map(|_| "goal")
                } else {
                    match tutorial_part(&b) {
                        Ok(_) => Ok("tutorial"),
                        Err(e) if e.to_string().contains("not a tutorial part") => continue,
                        Err(e) => Err(e),
                    }
                };
                match kind {
                    Ok(k) => *counts.entry(k).or_default() += 1,
                    Err(e) => panic!("{name}: {e}"),
                }
            }
        }
        assert_eq!(counts.get("achievement"), Some(&36), "{counts:?}");
        assert_eq!(counts.get("goal"), Some(&138), "{counts:?}");
        assert_eq!(counts.get("scenario"), Some(&4), "{counts:?}");
        assert_eq!(counts.get("tutorial"), Some(&557), "{counts:?}");
    }
}

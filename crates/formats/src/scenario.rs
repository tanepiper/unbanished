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

// +0x28; +0x20; +0x38
/// `Tutorial`: its steps, its window (a UIDescription template, opened at the right edge), the
/// scenario to start when it is done (none below 0).
///
/// Original: `FUN_1400e6240`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tutorial {
    /// The steps.
    pub steps: Vec<Option<String>>,
    /// The window (a UIDescription template, opened at the right edge).
    pub window: Option<String>,
    /// The scenario to start when it is done (none below 0).
    pub next: i32,
}

// +0x20; +0x28; +0x30; +0x38; +0x40; +0x50; +0x60
/// `TutorialStep`: a checkpoint (a loaded game resumes at the latest one), the string table, the
/// texts before and after, images, setups, goals.
///
/// Original: `FUN_1400e69f0`, `FUN_1400e7280`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TutorialStep {
    /// Whether this is a checkpoint (a loaded game resumes at the latest one).
    pub checkpoint: bool,
    /// The string table.
    pub strings: Option<String>,
    /// The text before.
    pub pre: String,
    /// The text after.
    pub post: String,
    /// The images.
    pub images: Vec<Option<String>>,
    /// The set-ups.
    pub setups: Vec<Option<String>>,
    /// The goals.
    pub goals: Vec<Option<String>>,
}

/// One part of a tutorial: the tutorial itself, a step, or a set-up dialog.
#[derive(Debug, Clone, PartialEq)]
pub enum TutorialPart {
    /// The tutorial.
    Tutorial(Tutorial),
    /// A step.
    Step(TutorialStep),
    // +0x20; +0x30; +0x40; +0x50; +0x48; +0x58; +0x70
    /// A set-up that limits the interface and points at an element.
    SetupDialog {
        /// The elements left usable.
        a: Vec<String>,
        /// More elements left usable.
        b: Vec<String>,
        /// The element to point at.
        s40: String,
        /// The highlight dialog.
        r50: Option<String>,
        /// The angle of the pointer.
        i48: i32,
        /// A ComponentDescription.
        component: String,
        /// Not identified yet.
        i70: i32,
    },
    /// Moves the camera to two cells.
    SetupCamera([i32; 4]),
    /// Sets the game speed.
    SetupGameSpeed {
        /// The speed.
        speed: i32,
        /// Whether it unpauses the game.
        b24: bool,
    },
    /// Limits what the player may place, and where.
    SetupRestrictPlayer {
        /// A rectangle of cells, then one more value.
        values: [i32; 5],
        /// The first template.
        s38: String,
        /// The second template.
        s50: String,
    },
    /// An element was pressed.
    GoalButton {
        /// The element's path.
        s20: String,
        /// Not identified yet.
        b28: bool,
    },
    /// A count reached a number.
    GoalCount {
        /// What is counted.
        e20: i32,
        /// The number.
        i24: i32,
        /// The template.
        s30: String,
        /// Whether the count must be at least the number (else exactly).
        b28: bool,
        /// The spinner's path, for kind 9.
        s48: String,
    },
    /// A dialog is open.
    GoalDialog {
        /// The dialog.
        s20: String,
        /// The template of the entity it is open on.
        component: String,
    },
    /// The game speed is this value.
    GoalGameSpeed(i32),
    // +0x28; +0x2c; +0x20
    /// A TutorialImage and its classes.
    Image {
        /// The column.
        i28: i32,
        /// The row.
        i2c: i32,
        /// The element.
        r20: Option<String>,
        /// What the class adds.
        extra: ImageExtra,
    },
}

/// What a tutorial image adds: a key name, an input key, or a toolbar icon.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageExtra {
    /// Nothing.
    None,
    /// A toolbar item's key.
    HotKey(String),
    /// A key-map action's key.
    InputKey(i32),
    /// A toolbar item's icon.
    ToolbarIcon(String),
}

/// Read a [`TutorialPart`] from the bytes of its resource.
pub fn tutorial_part(data: &[u8]) -> Result<TutorialPart> {
    let (_, v) = crs::parse(data, |r, h| {
        let t = h.type_id;
        let is = |n: &str| t == type_id(n);
        // A base class writes its own block first: TutorialGoal's is empty; TutorialImage's
        // holds the image.
        let goal = [
            "TutorialGoalButton",
            "TutorialGoalCount",
            "TutorialGoalDialog",
            "TutorialGoalGameSpeed",
        ]
        .iter()
        .any(|n| is(n));
        let image = [
            "TutorialImageHotKey",
            "TutorialImageInputKey",
            "TutorialImageToolbarIcon",
        ]
        .iter()
        .any(|n| is(n));
        if goal {
            r.block(|_, _| Ok(()))?;
        }
        let base = if image {
            Some(r.block(|r, _| Ok((r.i32()?, r.i32()?, r.external_ref()?)))?)
        } else {
            None
        };
        r.block(|r, _| {
            Ok(if is("Tutorial") {
                let steps = refs(r)?;
                TutorialPart::Tutorial(Tutorial {
                    steps,
                    window: r.external_ref()?,
                    next: r.i32()?,
                })
            } else if is("TutorialStep") {
                TutorialPart::Step(TutorialStep {
                    checkpoint: r.bool()?,
                    strings: r.external_ref()?,
                    pre: r.string()?,
                    post: r.string()?,
                    images: refs(r)?,
                    setups: refs(r)?,
                    goals: refs(r)?,
                })
            } else if is("TutorialSetupDialog") {
                let a = (0..r.count()?).map(|_| r.string()).collect::<Result<_>>()?;
                let b = (0..r.count()?).map(|_| r.string()).collect::<Result<_>>()?;
                TutorialPart::SetupDialog {
                    a,
                    b,
                    s40: r.string()?,
                    r50: r.external_ref()?,
                    i48: r.i32()?,
                    component: r.string()?,
                    i70: r.i32()?,
                }
            } else if is("TutorialSetupCamera") {
                TutorialPart::SetupCamera([r.i32()?, r.i32()?, r.i32()?, r.i32()?])
            } else if is("TutorialSetupGameSpeed") {
                TutorialPart::SetupGameSpeed {
                    speed: r.i32()?,
                    b24: r.bool()?,
                }
            } else if is("TutorialSetupRestrictPlayer") {
                TutorialPart::SetupRestrictPlayer {
                    values: [r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?],
                    s38: r.string()?,
                    s50: r.string()?,
                }
            } else if is("TutorialGoalButton") {
                TutorialPart::GoalButton {
                    s20: r.string()?,
                    b28: r.bool()?,
                }
            } else if is("TutorialGoalCount") {
                TutorialPart::GoalCount {
                    e20: r.i32()?,
                    i24: r.i32()?,
                    s30: r.string()?,
                    b28: r.bool()?,
                    s48: r.string()?,
                }
            } else if is("TutorialGoalDialog") {
                TutorialPart::GoalDialog {
                    s20: r.string()?,
                    component: r.string()?,
                }
            } else if is("TutorialGoalGameSpeed") {
                TutorialPart::GoalGameSpeed(r.i32()?)
            } else if is("TutorialImage")
                || is("TutorialImageHotKey")
                || is("TutorialImageInputKey")
                || is("TutorialImageToolbarIcon")
            {
                let (i28, i2c, r20) = match base.clone() {
                    Some(b) => b,
                    None => (r.i32()?, r.i32()?, r.external_ref()?),
                };
                let extra = if is("TutorialImageHotKey") {
                    ImageExtra::HotKey(r.string()?)
                } else if is("TutorialImageInputKey") {
                    ImageExtra::InputKey(r.i32()?)
                } else if is("TutorialImageToolbarIcon") {
                    ImageExtra::ToolbarIcon(r.string()?)
                } else {
                    ImageExtra::None
                };
                TutorialPart::Image {
                    i28,
                    i2c,
                    r20,
                    extra,
                }
            } else {
                return format_err(format!("not a tutorial part: {t:#010x}"));
            })
        })
    })?;
    Ok(v)
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

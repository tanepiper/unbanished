//! Achievements, scenarios and tutorials (`Game/Achievement`, `Game/Scenario`). See
//! docs/ACHIEVEMENTS.md and docs/TUTORIAL.md.
//!
//! - `Achievement::Achievement` (Runtime `Serialize` `0x18009eb70`): its string table, title,
//!   description, sprite sheet and sprite, and its goals.
//! - `Achievement::Goal` (`0x18009fd30`): a string table and text; then each class's own
//!   block: BuildingGoal (`FUN_1400dcb70`), PopulationGoal (`FUN_1400dcd40`), StatGoal
//!   (`FUN_1400dfd80`), TimeGoal (`FUN_1400dfff0`), FinishTutorialGoal (`FUN_1400dcea0`).
//! - `Scenario` (`FUN_1400ae840`): an achievement, then the game it starts.
//! - `Tutorial` (`FUN_1400e3fa0`) and its steps, setups, goals and images.

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Result};

/// `Achievement::Achievement`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Achievement {
    /// `+0x20`.
    pub strings: Option<String>,
    /// `+0x28`, `+0x30`: the title's and description's keys.
    pub title: String,
    pub description: String,
    /// `+0x38`, `+0x40`.
    pub sprites: Option<String>,
    pub sprite: String,
    /// `+0x48`: the goals.
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
    /// `+0x30` all of them (else any), `+0x38` the buildings (ComponentDescriptions).
    Building { all: bool, buildings: Vec<String> },
    /// `+0x30` citizens.
    Population(i32),
    /// `+0x30..` three enums, an amount, a flag, two references, an enum and an amount.
    Stat {
        e30: i32,
        e34: i32,
        e38: i32,
        amount: i32,
        flag: bool,
        r48: String,
        r60: String,
        e78: i32,
        i7c: i32,
    },
    /// `+0x30` years, `+0x34` (within that time, else after it).
    Time { years: i32, within: bool },
    /// `+0x30` the tutorial.
    FinishTutorial(Option<String>),
}

/// `Achievement::Goal`.
#[derive(Debug, Clone, PartialEq)]
pub struct Goal {
    pub strings: Option<String>,
    pub text: String,
    pub kind: GoalKind,
}

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
    pub achievement: Achievement,
    /// `+0x60`, `+0x64`.
    pub i60: i32,
    pub i64: i32,
    /// `+0x58`: a save to start from.
    pub save: String,
    /// `+0x70`, `+0x78`, `+0x80`.
    pub r70: Option<String>,
    pub r78: Option<String>,
    pub r80: Option<String>,
    /// `+0x88`.
    pub s88: String,
    /// `+0x68`, `+0x69`.
    pub b68: bool,
    pub b69: bool,
}

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
                save: r.string()?,
                r70: r.external_ref()?,
                r78: r.external_ref()?,
                r80: r.external_ref()?,
                s88: r.string()?,
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

/// `Tutorial`: `+0x28` its steps, `+0x20` its window (a UIDescription template, opened at the
/// right edge), `+0x38` the scenario to start when it is done (`FUN_1400e6240`; none below 0).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tutorial {
    pub steps: Vec<Option<String>>,
    pub window: Option<String>,
    pub next: i32,
}

/// `TutorialStep`: `+0x20` a checkpoint (a loaded game resumes at the latest one,
/// `FUN_1400e69f0`), `+0x28` the string table, `+0x30`, `+0x38` the texts before and after,
/// `+0x40` images, `+0x50` setups, `+0x60` goals (`FUN_1400e7280`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TutorialStep {
    pub checkpoint: bool,
    pub strings: Option<String>,
    pub pre: String,
    pub post: String,
    pub images: Vec<Option<String>>,
    pub setups: Vec<Option<String>>,
    pub goals: Vec<Option<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TutorialPart {
    Tutorial(Tutorial),
    Step(TutorialStep),
    /// `+0x20`, `+0x30` string lists, `+0x40` a string, `+0x50` a reference, `+0x48`, a
    /// ComponentDescription path `+0x58`, `+0x70`.
    SetupDialog {
        a: Vec<String>,
        b: Vec<String>,
        s40: String,
        r50: Option<String>,
        i48: i32,
        component: String,
        i70: i32,
    },
    SetupCamera([i32; 4]),
    SetupGameSpeed {
        speed: i32,
        b24: bool,
    },
    SetupRestrictPlayer {
        values: [i32; 5],
        s38: String,
        s50: String,
    },
    GoalButton {
        s20: String,
        b28: bool,
    },
    GoalCount {
        e20: i32,
        i24: i32,
        s30: String,
        b28: bool,
        s48: String,
    },
    GoalDialog {
        s20: String,
        component: String,
    },
    GoalGameSpeed(i32),
    /// TutorialImage and its classes: `+0x28`, `+0x2c`, `+0x20` a reference, then the
    /// class's own (a toolbar path, a key).
    Image {
        i28: i32,
        i2c: i32,
        r20: Option<String>,
        extra: ImageExtra,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageExtra {
    None,
    HotKey(String),
    InputKey(i32),
    ToolbarIcon(String),
}

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

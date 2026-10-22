//! Tutorials: their steps and the parts (text, images, goals) a step shows.

use super::*;

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

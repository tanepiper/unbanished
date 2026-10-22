//! A tutorial's definition as the game data gives it: its steps, their setup, goals and images, read from the tutorial's parts.

use super::*;

/// A step's setup (`TutorialSetup`).
#[derive(Debug, Clone)]
pub enum Setup {
    Dialog {
        enable: Vec<String>,
        close: Vec<String>,
        point: String,
        highlight: Option<String>,
        angle: i32,
        component: String,
        index: i32,
    },
    Camera([i32; 4]),
    Speed {
        speed: i32,
        unpause: bool,
    },
    Restrict {
        rect: [i32; 4],
        rotation: i32,
        place: String,
        select: String,
    },
}

/// A step's goal (`TutorialGoal`).
#[derive(Debug, Clone)]
pub enum Goal {
    Button(String),
    Count {
        kind: i32,
        amount: i32,
        reference: String,
        at_least: bool,
        path: String,
    },
    Dialog {
        path: String,
        component: String,
    },
    Speed(i32),
}

#[derive(Debug, Clone)]
pub struct Image {
    pub column: i32,
    pub row: i32,
    pub element: Option<String>,
    pub extra: ImageExtra,
}

#[derive(Debug, Clone, Default)]
pub struct Step {
    /// Read when a saved game resumes (`FUN_1400e69f0`).
    #[allow(dead_code)]
    pub checkpoint: bool,
    pub strings: Option<String>,
    pub pre: String,
    pub post: String,
    pub images: Vec<Image>,
    pub setups: Vec<Setup>,
    pub goals: Vec<Goal>,
}

#[derive(Debug, Clone, Default)]
pub struct TutorialDef {
    pub steps: Vec<Step>,
    pub window: String,
    pub next: i32,
}

pub(super) fn part(archive: &Archive, r: &str) -> Option<TutorialPart> {
    scenario::tutorial_part(&archive.read_ref(&r.replace('\\', "/")).ok()?).ok()
}

/// The tutorial and its steps' parts.
pub fn load(archive: &Archive, reference: &str) -> Option<TutorialDef> {
    let TutorialPart::Tutorial(t) = part(archive, reference)? else {
        return None;
    };
    let mut steps = Vec::new();
    for s in t.steps.iter().flatten() {
        let Some(TutorialPart::Step(s)) = part(archive, s) else {
            continue;
        };
        let mut step = Step {
            checkpoint: s.checkpoint,
            strings: s.strings.clone(),
            pre: s.pre.clone(),
            post: s.post.clone(),
            ..Default::default()
        };
        for r in s.images.iter().chain(&s.setups).chain(&s.goals).flatten() {
            match part(archive, r) {
                Some(TutorialPart::Image {
                    i28,
                    i2c,
                    r20,
                    extra,
                }) => step.images.push(Image {
                    column: i28,
                    row: i2c,
                    element: r20,
                    extra,
                }),
                Some(TutorialPart::SetupDialog {
                    a,
                    b,
                    s40,
                    r50,
                    i48,
                    component,
                    i70,
                }) => step.setups.push(Setup::Dialog {
                    enable: a,
                    close: b,
                    point: s40,
                    highlight: r50,
                    angle: i48,
                    component,
                    index: i70,
                }),
                Some(TutorialPart::SetupCamera(c)) => step.setups.push(Setup::Camera(c)),
                Some(TutorialPart::SetupGameSpeed { speed, b24 }) => {
                    step.setups.push(Setup::Speed {
                        speed,
                        unpause: b24,
                    })
                }
                Some(TutorialPart::SetupRestrictPlayer { values, s38, s50 }) => {
                    step.setups.push(Setup::Restrict {
                        rect: [values[0], values[1], values[2], values[3]],
                        rotation: values[4],
                        place: s38,
                        select: s50,
                    })
                }
                Some(TutorialPart::GoalButton { s20, .. }) => step.goals.push(Goal::Button(s20)),
                Some(TutorialPart::GoalCount {
                    e20,
                    i24,
                    s30,
                    b28,
                    s48,
                }) => step.goals.push(Goal::Count {
                    kind: e20,
                    amount: i24,
                    reference: s30,
                    at_least: b28,
                    path: s48,
                }),
                Some(TutorialPart::GoalDialog { s20, component }) => {
                    step.goals.push(Goal::Dialog {
                        path: s20,
                        component,
                    })
                }
                Some(TutorialPart::GoalGameSpeed(s)) => step.goals.push(Goal::Speed(s)),
                _ => {}
            }
        }
        steps.push(step);
    }
    Some(TutorialDef {
        steps,
        window: t.window.unwrap_or_default(),
        next: t.next,
    })
}

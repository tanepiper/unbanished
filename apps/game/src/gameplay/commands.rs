//! Player commands: every action of a player that changes the game, as one value, carried out by
//! one function, [`GameplayLoop::execute`].
//!
//! In a single-player game each command is carried out at once, at the point where the action's
//! code ran before there were commands: after the window event, inside the drag, at the click.
//! So the game changes in the same order as before, random draws included, and saves and frames
//! are unchanged (`tools/compat/compare.sh`, the `actions` scenario). Commands are data that can
//! be written down and read back: a recording replays a game exactly
//! ([`super::recording`]), and a multiplayer game could carry each command out on every computer
//! at the same update (docs/MULTIPLAYER-PLAN.md).

use serde::{Deserialize, Serialize};
use unbanished_formats::pkg::Archive;
use unbanished_game::work::professions::Command;

use super::{GameplayLoop, SPEED_LEVELS};
use crate::tools::path_tool::PathTool;
use crate::tools::placement_tool::{self, Placement, PlacementTool};
use crate::tools::removal_tool::{DragSound, RemovalTool};
use crate::tools::selection::Entity;

/// One action of a player that changes the game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerCommand {
    /// A building window's, the Professions window's or a tool's command for the town's queue,
    /// applied by the next iteration or by [`PlayerCommand::ApplyQueued`].
    Town(Command),
    /// The queued commands take effect now: the original's window handlers and tools act inside
    /// the input handler, paused or not. After a window event the crop fields' calendar and the
    /// weather are refreshed first (`with_calendar`); after a drag they are not.
    ApplyQueued { with_calendar: bool },
    /// UpgradeUI (`FUN_14008c160`): load the template the building becomes, then queue the
    /// upgrade.
    Upgrade { entity: usize, template: String },
    /// A tool starts (`FUN_140108030`): the template and those it uses are loaded, and a
    /// placement tool draws its first variant from the game's `Random` (`FUN_140103a50`).
    StartTool { template: String },
    /// PlacementTool's click (`FUN_140102b20`): the building is created.
    Place(Placement),
    /// A drag tool's apply (`FUN_140108ab0`; the road, bridge, area and fence tools).
    Drag(Drag),
    /// The Path tool's pick (`FUN_1400e3b80`): the entity's paths are searched, which draws from
    /// the game's `Random`.
    PathQuery { entity: Entity },
    /// The Map window's new-name button (`FUN_14006c060`): a town name of at most `max_length`
    /// characters, drawn from the game's `Random`.
    RandomTownName { max_length: i32 },
    /// The Map window's OK: the town's new name.
    RenameTown { name: String },
    /// A game speed (an index into the toolbar's levels: 1, 2, 5, 10).
    SetSpeed(u8),
    /// Pause or play.
    SetPaused(bool),
}

/// A drag: the tool's template, and the cells and settings it was applied with. The tool is the
/// one [`PlayerCommand::StartTool`] made for that template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drag {
    pub template: String,
    pub start: i32,
    pub end: i32,
    pub rotation: i32,
    pub restrict: Option<[i32; 4]>,
    pub manhattan: bool,
}

/// What a command gives back to the player who gave it.
pub enum Outcome {
    None,
    /// The tool that started, if the template has one.
    Tool(Option<Tool>),
    /// The building created.
    Placed(usize),
    /// The sound of an applied drag.
    Sound(DragSound),
    /// A town name.
    Name(String),
    /// A multiplayer game: the command waits for its lockstep slot, and its outcome comes then
    /// ([`GameplayLoop::local_outcome`]).
    Queued,
}

/// A placement, drag or path tool.
pub enum Tool {
    Placement(PlacementTool),
    Removal(RemovalTool),
    Path(PathTool),
}

impl GameplayLoop {
    /// The town names the Map window draws from (`Game/Names/Cities_resource.crs`), read the
    /// first time they are wanted.
    pub(super) fn load_town_names(
        &mut self,
        archive: &Archive,
    ) -> Option<&unbanished_formats::names::NameDatabase> {
        if self.town_names.is_none() {
            self.town_names = archive
                .read("Game/Names/Cities_resource.crs")
                .and_then(|b| unbanished_formats::names::NameDatabase::read(&b))
                .map_err(|e| log::error!("town names: {e}"))
                .ok();
        }
        self.town_names.as_ref()
    }

    /// Carries out a player's command now, and writes it to the recording if there is one.
    pub(super) fn execute(&mut self, archive: &Archive, command: PlayerCommand) -> Outcome {
        // In lockstep the player's commands wait for their slot, when every computer carries
        // them out in the same order.
        if !self.carrying_out {
            if let Some(l) = &mut self.lockstep {
                l.give(self.updates, command);
                return Outcome::Queued;
            }
        }
        if let Some(r) = &mut self.recorder {
            r.command(self.updates, &command);
        }
        match command {
            PlayerCommand::Town(command) => {
                if let Some(town) = &mut self.town {
                    town.construction.commands.push(command);
                }
                Outcome::None
            }
            PlayerCommand::ApplyQueued {
                with_calendar: true,
            } => {
                self.apply_commands();
                Outcome::None
            }
            PlayerCommand::ApplyQueued {
                with_calendar: false,
            } => {
                if let (Some(town), Some(view)) = (&mut self.town, &self.view) {
                    let grid = unbanished_game::terrain::HeightGrid {
                        width: view.terrain.size + 1,
                        heights: &view.terrain.heights,
                    };
                    town.apply_commands(&grid, &mut self.rng);
                }
                Outcome::None
            }
            PlayerCommand::Upgrade { entity, template } => {
                if let Some(town) = &mut self.town {
                    if let Some(t) = town.load_template(archive, &template) {
                        town.site_template(archive, t);
                        town.construction.commands.push(Command::Upgrade {
                            entity,
                            template: t,
                        });
                    }
                }
                Outcome::None
            }
            PlayerCommand::StartTool { template } => {
                let Some(town) = &mut self.town else {
                    return Outcome::Tool(None);
                };
                let tool = match PlacementTool::start(archive, town, &template, &mut self.rng) {
                    Some(t) => Some(Tool::Placement(t)),
                    None => match RemovalTool::start(archive, town, &template) {
                        Some(t) => {
                            self.drag_tools.insert(template.clone(), t.clone());
                            Some(Tool::Removal(t))
                        }
                        None => PathTool::start(archive, &template).map(Tool::Path),
                    },
                };
                Outcome::Tool(tool)
            }
            PlayerCommand::Place(p) => match &mut self.town {
                Some(town) => Outcome::Placed(placement_tool::place(archive, town, &p)),
                None => Outcome::None,
            },
            PlayerCommand::Drag(drag) => {
                let (Some(tool), Some(town), Some(view)) = (
                    self.drag_tools.get(&drag.template),
                    &mut self.town,
                    &self.view,
                ) else {
                    return Outcome::None;
                };
                let mut tool = tool.with_drag(&drag);
                let grid = unbanished_game::terrain::HeightGrid {
                    width: view.terrain.size + 1,
                    heights: &view.terrain.heights,
                };
                Outcome::Sound(tool.apply(archive, town, &grid))
            }
            PlayerCommand::PathQuery { entity } => {
                if let (Some(tool), Some(town)) = (self.path_tool.as_mut(), self.town.as_mut()) {
                    tool.pick(town, &mut self.rng, entity);
                }
                Outcome::None
            }
            PlayerCommand::RandomTownName { max_length } => match {
                self.load_town_names(archive);
                self.town_names.as_ref()
            } {
                Some(db) => Outcome::Name(unbanished_game::people::names::town_name(
                    db,
                    &mut self.rng,
                    max_length / 3 * 2,
                )),
                None => Outcome::None,
            },
            PlayerCommand::RenameTown { name } => {
                self.params.town_name = name;
                Outcome::None
            }
            PlayerCommand::SetSpeed(speed) => {
                self.speed = speed.min((SPEED_LEVELS.len() - 1) as u8);
                Outcome::None
            }
            PlayerCommand::SetPaused(paused) => {
                self.paused = paused;
                Outcome::None
            }
        }
    }
}

//! Player commands: every action of a player that changes the game, as one value, carried out by
//! one function, [`GameplayLoop::execute`].
//!
//! In a single-player game each command is carried out at once, at the point where the action's
//! code ran before there were commands: after the window event, inside the drag, at the click.
//! So the game changes in the same order as before, random draws included, and saves and frames
//! are unchanged (`tools/compat/compare.sh`, the `actions` scenario). A multiplayer game could
//! instead carry a command out on every computer at the same update; see
//! docs/MULTIPLAYER-PLAN.md. Commands hold data, not references to the player's tools, so that
//! they can be sent.

use banished_formats::pkg::Archive;
use banished_game::work::professions::Command;

use super::{GameplayLoop, SPEED_LEVELS};
use crate::tools::placement_tool::{self, Placement, PlacementTool};
use crate::tools::removal_tool::{DragSound, RemovalTool};
use crate::tools::selection::Entity;

/// One action of a player that changes the game.
pub enum PlayerCommand {
    /// A building window's, the Professions window's or a tool's command for the town's queue,
    /// applied by the next iteration or right after the window event.
    Town(Command),
    /// UpgradeUI (`FUN_14008c160`): load the template the building becomes, then queue the
    /// upgrade.
    Upgrade { entity: usize, template: String },
    /// A tool starts (`FUN_140108030`): the template and those it uses are loaded, and a
    /// placement tool draws its first variant from the game's `Random` (`FUN_140103a50`).
    StartTool { template: String },
    /// PlacementTool's click (`FUN_140102b20`): the building is created.
    Place(Placement),
    /// A drag tool's apply (`FUN_140108ab0`; the road, bridge, area and fence tools): the tool
    /// as it stood when the drag ended.
    Drag(Box<RemovalTool>),
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
}

/// A placement or drag tool.
pub enum Tool {
    Placement(PlacementTool),
    Removal(RemovalTool),
}

impl GameplayLoop {
    /// Carries out a player's command now.
    pub(super) fn execute(&mut self, archive: &Archive, command: PlayerCommand) -> Outcome {
        match command {
            PlayerCommand::Town(command) => {
                if let Some(town) = &mut self.town {
                    town.construction.commands.push(command);
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
                    None => RemovalTool::start(archive, town, &template).map(Tool::Removal),
                };
                Outcome::Tool(tool)
            }
            PlayerCommand::Place(p) => match &mut self.town {
                Some(town) => Outcome::Placed(placement_tool::place(archive, town, &p)),
                None => Outcome::None,
            },
            PlayerCommand::Drag(mut tool) => {
                let (Some(town), Some(view)) = (&mut self.town, &self.view) else {
                    return Outcome::None;
                };
                let grid = banished_game::terrain::HeightGrid {
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
            PlayerCommand::RandomTownName { max_length } => match self.town_names.as_ref() {
                Some(db) => Outcome::Name(banished_game::people::names::town_name(
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

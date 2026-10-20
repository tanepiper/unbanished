//! Player commands: every action of a player that changes the game, as one value, carried out by
//! one function, [`Simulation::execute`] (the game app handles the speed, the pause and the
//! town's new name itself, and routes commands through recordings and multiplayer first).
//!
//! In a single-player game each command is carried out at once, at the point where the action's
//! code ran before there were commands: after the window event, inside the drag, at the click.
//! So the game changes in the same order as before, random draws included, and saves and frames
//! are unchanged (`tools/compat/compare.sh`, the `actions` scenario). Commands are data that can
//! be written down and read back: a recording replays a game exactly, and a multiplayer game
//! carries each command out on every computer at the same update (docs/MULTIPLAYER-PLAN.md).

use serde::{Deserialize, Serialize};
use unbanished_formats::pkg::Archive;
use unbanished_game::work::professions::Command;

use crate::simulation::Simulation;
use crate::tools::drag::Drag;
use crate::tools::entity::Entity;
use crate::tools::path_tool::PathTool;
use crate::tools::placement_tool::{self, Placement, PlacementTool};
use crate::tools::removal_tool::{DragSound, RemovalTool};

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
    /// UpgradeUI: load the template the building becomes, then queue the upgrade.
    ///
    /// Original: `FUN_14008c160`.
    Upgrade { entity: usize, template: String },
    /// A tool starts: the template and those it uses are loaded, and a placement tool draws its
    /// first variant from the game's `Random`.
    ///
    /// Original: `FUN_140108030`, `FUN_140103a50`.
    StartTool { template: String },
    /// PlacementTool's click: the building is created.
    ///
    /// Original: `FUN_140102b20`.
    Place(Placement),
    /// A drag tool's apply (the road, bridge, area and fence tools).
    ///
    /// Original: `FUN_140108ab0`.
    Drag(Drag),
    /// The Path tool's pick: the entity's paths are searched, which draws from the game's `Random`.
    ///
    /// Original: `FUN_1400e3b80`.
    PathQuery { entity: Entity },
    /// The Map window's new-name button: a town name of at most `max_length` characters, drawn from
    /// the game's `Random`.
    ///
    /// Original: `FUN_14006c060`.
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
    /// A multiplayer game: the command waits for its lockstep slot, and its outcome comes then
    /// (the game app's `GameplayLoop::local_outcome`).
    Queued,
}

/// A placement, drag or path tool.
pub enum Tool {
    Placement(PlacementTool),
    Removal(RemovalTool),
    Path(PathTool),
}

impl Simulation {
    /// The town names the Map window draws from (`Game/Names/Cities_resource.crs`), read the
    /// first time they are wanted.
    pub fn load_town_names(
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

    /// Carries out a player's command now. `path_tool` is the Path tool a `PathQuery` picks
    /// with. `RenameTown`, `SetSpeed` and `SetPaused` are the game app's and do nothing here.
    pub fn execute(
        &mut self,
        archive: &Archive,
        command: PlayerCommand,
        path_tool: Option<&mut PathTool>,
    ) -> Outcome {
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
                if let (Some(town), Some(heights)) = (&mut self.town, &self.heights) {
                    let grid = heights.grid();
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
                let (Some(tool), Some(town), Some(heights)) = (
                    self.drag_tools.get(&drag.template),
                    &mut self.town,
                    &self.heights,
                ) else {
                    return Outcome::None;
                };
                let mut tool = tool.with_drag(&drag);
                let grid = heights.grid();
                Outcome::Sound(tool.apply(archive, town, &grid))
            }
            PlayerCommand::PathQuery { entity } => {
                if let (Some(tool), Some(town)) = (path_tool, self.town.as_mut()) {
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
            PlayerCommand::RenameTown { .. }
            | PlayerCommand::SetSpeed(_)
            | PlayerCommand::SetPaused(_) => Outcome::None,
        }
    }
}

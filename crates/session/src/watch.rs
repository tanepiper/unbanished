//! A room's town run by the server (not in the original): the server's own copy of a multiplayer
//! game. It gets every player's messages (the start, the turns, new players, resyncs) and runs
//! the town update by update as each player's game does, with no window. Its digests are the
//! game everyone must have; when a player's game differs, the server sends its own save
//! ([`Event::Save`]) and the players go on from it.
//!
//! It plays no part in the lockstep: it sends no turns, so it never holds the players up. Each
//! update repeats the game app's frame in a lockstep game (`gameplay/frame.rs`): the slot's
//! commands (speed, pause and the town's name as the app handles them, and each player's own
//! Path tool), then the update boundary (a save if one is wanted, a digest on check updates),
//! then the speed's iterations, the achievements, the phase after them and the events
//! (`Simulation::frame`).
//!
//! Deviation: not in the original, which has no multiplayer.

use std::collections::BTreeMap;
use std::sync::Arc;

use unbanished_formats::pkg::Archive;
use unbanished_sim::command::{Outcome, PlayerCommand, Tool};
use unbanished_sim::saved::SavedGame;
use unbanished_sim::simulation::Simulation;
use unbanished_sim::strings::ArchiveStrings;
use unbanished_sim::tools::path_tool::PathTool;
use unbanished_sim::tools::removal_tool::RemovalTool;
use unbanished_sim::town::world_params::WorldParams;

use crate::lockstep::Message;
use crate::session::{LockstepSession, Session};

/// The game speeds: iterations per update for each of the toolbar's levels (1, 2, 5, 10).
pub const SPEED_LEVELS: [u32; 4] = [1, 2, 5, 10];

/// One update's game time: `Engine::MainLoop`'s step, 1/60 s in ticks.
const FRAME_STEP: i64 = 16_666;

/// The watcher's player number in its lockstep: none of the room's.
const WATCHER: u8 = u8::MAX;

/// What the watcher has for the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The digest of the town when `update` updates had begun, made in `epoch`.
    Digest { epoch: u32, update: u64, crc: u32 },
    /// The town's save at `update`, for the players to go on from (a resync).
    Save { update: u64, bytes: Vec<u8> },
}

/// The game being watched.
struct Watched {
    sim: Simulation,
    saved: SavedGame,
    params: WorldParams,
    lockstep: LockstepSession,
    speed: u8,
    paused: bool,
    /// Each player's Path tool (a pick depends on the one before).
    paths: BTreeMap<u8, PathTool>,
    /// How often the players' digests are compared (0: never).
    check: u64,
    /// The update boundary handled last.
    boundary_done: Option<u64>,
    /// The server asked for a save at the next update boundary.
    save_wanted: bool,
}

/// A room's town, run by the server.
pub struct Watcher {
    archive: Arc<Archive>,
    game: Option<Watched>,
}

impl Watcher {
    /// A watcher with the server's game data, waiting for the game's start.
    pub fn new(archive: Arc<Archive>) -> Watcher {
        Watcher {
            archive,
            game: None,
        }
    }

    /// Whether the game has started.
    pub fn started(&self) -> bool {
        self.game.is_some()
    }

    /// The updates begun so far.
    pub fn updates(&self) -> u64 {
        self.game.as_ref().map_or(0, |g| g.sim.updates)
    }

    /// A message a player sent the room.
    pub fn receive(&mut self, message: Message) -> Result<(), String> {
        match message {
            // The start of the game (a player joining a running game gets a start of its own,
            // while everybody else reloads the same save: that is the `Save`).
            Message::Start {
                players,
                delay,
                check,
                absent,
                bytes,
                update: 0,
                ..
            } if self.game.is_none() => {
                let mut lockstep = LockstepSession::new(WATCHER, players, delay);
                for p in absent {
                    lockstep.lockstep.leave(p);
                }
                let (sim, saved) = self.load(&bytes)?;
                self.game = Some(Watched {
                    params: saved.params(),
                    sim,
                    saved,
                    lockstep,
                    speed: 0,
                    paused: false,
                    paths: BTreeMap::new(),
                    check,
                    boundary_done: None,
                    save_wanted: false,
                });
                log::info!("watching a game of {players} players");
            }
            Message::Save { update, bytes } => self.reload(update, &bytes)?,
            Message::Turn(turn) => {
                if let Some(g) = &mut self.game {
                    g.lockstep.lockstep.receive(turn);
                }
            }
            Message::Grow { player, from } => {
                if let Some(g) = &mut self.game {
                    g.lockstep.lockstep.add(player, from);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// A player left: its turns are not waited for after its last one.
    pub fn left(&mut self, player: u8) {
        if let Some(g) = &mut self.game {
            g.lockstep.lockstep.leave(player);
        }
    }

    /// The server wants the town's save at the next update boundary.
    pub fn want_save(&mut self) {
        if let Some(g) = &mut self.game {
            g.save_wanted = true;
        }
    }

    /// Runs every update whose turns are all here; what it has for the server.
    pub fn advance(&mut self) -> Result<Vec<Event>, String> {
        let mut events = Vec::new();
        let dt = unbanished_game::scheduler::seconds(FRAME_STEP);
        loop {
            let archive = self.archive.clone();
            let Some(g) = self.game.as_mut() else {
                break;
            };
            let update = g.sim.updates;
            let Some(commands) = g.lockstep.slot(update) else {
                break;
            };
            g.lockstep.take_outbox();
            for (player, command) in commands {
                g.execute(&archive, player, command);
            }
            if g.boundary_done != Some(update) {
                g.boundary_done = Some(update);
                if std::mem::take(&mut g.save_wanted) {
                    let bytes = g.save_file(&archive);
                    events.push(Event::Save {
                        update,
                        bytes: bytes.clone(),
                    });
                    // As every player does with a resync's save: go on from the save itself.
                    self.reload(update, &bytes)?;
                    continue;
                }
                if g.check > 0 && update > 0 && update % g.check == 0 {
                    let crc = g.digest(&archive);
                    let epoch = g.lockstep.lockstep.epoch;
                    events.push(Event::Digest { epoch, update, crc });
                }
            }
            g.sim.updates += 1;
            let iterations = if g.paused {
                0
            } else {
                SPEED_LEVELS[g.speed as usize]
            };
            let mut strings = ArchiveStrings::new(&archive);
            // In a multiplayer game AutoSlow is always on.
            let frame = g
                .sim
                .frame(&archive, &mut strings, &g.params, iterations, dt, true);
            if frame.slow {
                g.speed = 0;
            }
        }
        Ok(events)
    }

    /// A save's town, loaded as each player's game loads it.
    fn load(&self, bytes: &[u8]) -> Result<(Simulation, SavedGame), String> {
        let saved = SavedGame::parse(bytes).map_err(|e| e.to_string())?;
        let mut sim = Simulation::from_save(&self.archive, &saved).map_err(|e| e.to_string())?;
        // As in every player's multiplayer game: the achievements count from none won.
        sim.load_achievements(&self.archive, Some(&saved), |_| false);
        Ok((sim, saved))
    }

    /// The game goes on from a resync's save made at `update`, as each player's does: the
    /// lockstep, the speed and the pause go on, the drag tools start again by template on the
    /// new town, and the Path tools start afresh.
    fn reload(&mut self, update: u64, bytes: &[u8]) -> Result<(), String> {
        let (mut sim, saved) = self.load(bytes)?;
        let Some(g) = self.game.as_mut() else {
            return Ok(());
        };
        let tools: Vec<String> = std::mem::take(&mut g.sim.drag_tools).into_keys().collect();
        if let Some(town) = sim.town.as_mut() {
            for template in tools {
                if let Some(t) = RemovalTool::start(&self.archive, town, &template) {
                    sim.drag_tools.insert(template, t);
                }
            }
        }
        sim.updates = update;
        g.params = saved.params();
        g.sim = sim;
        g.saved = saved;
        g.paths.clear();
        g.lockstep.resynced(update);
        g.lockstep.take_outbox();
        g.boundary_done = Some(update);
        Ok(())
    }
}

impl Watched {
    /// A player's command, carried out as every player's game carries out another player's.
    fn execute(&mut self, archive: &Archive, player: u8, command: PlayerCommand) {
        match command {
            PlayerCommand::RenameTown { name } => self.params.town_name = name,
            PlayerCommand::SetSpeed(speed) => {
                self.speed = speed.min((SPEED_LEVELS.len() - 1) as u8);
            }
            PlayerCommand::SetPaused(paused) => self.paused = paused,
            command => {
                let mut tool = self.paths.remove(&player);
                if let Outcome::Tool(Some(Tool::Path(t))) =
                    self.sim.execute(archive, command, tool.as_mut())
                {
                    tool = Some(t);
                }
                if let Some(t) = tool {
                    self.paths.insert(player, t);
                }
            }
        }
    }

    /// The town's state, as each player's game encodes it for a digest.
    fn state(&self, archive: &Archive) -> unbanished_formats::savegame::GameState {
        let mut strings = ArchiveStrings::new(archive);
        self.sim
            .saved_state(archive, &mut strings, &self.saved, &self.params.town_name)
    }

    /// The digest: the CRC-32 of the encoded state, with each player's own view (the
    /// buildings' windows) left out, as each player's digest leaves it out.
    fn digest(&self, archive: &Archive) -> u32 {
        let mut state = self.state(archive);
        for object in state.objects.values_mut() {
            if object.type_name == "UIComponent" {
                object.fields.set(
                    "ui.pages",
                    unbanished_formats::savegame::Value::List(Vec::new()),
                );
            }
        }
        unbanished_formats::crc::crc32(state.encode().unwrap_or_default())
    }

    /// The town as a save file.
    fn save_file(&self, archive: &Archive) -> Vec<u8> {
        let state = self.state(archive).encode().unwrap_or_default();
        let mut metadata = self.saved.metadata.clone();
        metadata.name = self.params.town_name.clone();
        metadata.mods = Vec::new();
        unbanished_formats::save::Save::write(&metadata, &state)
    }
}

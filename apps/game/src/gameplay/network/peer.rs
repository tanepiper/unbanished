//! One player's game in a multiplayer session, driven a step at a time.

use super::*;

/// One player's game in a multiplayer session, driven a step at a time (the tests, and the
/// browser's network page). Player 0 is the host: it compares the players' digests and resyncs
/// everybody from its save when they differ.
pub struct NetPeer {
    pub player: u8,
    /// The game, once started (a joining player waits for the host's [`Message::Start`]).
    pub game: Option<GameplayLoop>,
    /// The link until the game starts; then the game holds it.
    link: Option<Box<dyn Link>>,
    check: u64,
    /// What arrived before the start (a running game's turns, for a player who joins it).
    early: Vec<(u8, Message)>,
}

impl NetPeer {
    pub fn new(player: u8, link: Box<dyn Link>, check: u64) -> NetPeer {
        NetPeer {
            player,
            game: None,
            link: Some(link),
            check,
            early: Vec::new(),
        }
    }

    /// A peer whose game is already loaded and joined to its network (the in-process test).
    pub fn with_game(player: u8, mut game: GameplayLoop) -> NetPeer {
        if let Some(net) = &mut game.online.net {
            net.driven = true;
        }
        NetPeer {
            player,
            game: Some(game),
            link: None,
            check: 0,
            early: Vec::new(),
        }
    }

    /// The host starts the game for `players` from its save: everybody, the host included,
    /// loads the same bytes.
    pub fn host_start(
        &mut self,
        ctx: &mut Ctx,
        bytes: Vec<u8>,
        players: u8,
        delay: u64,
    ) -> Result<(), String> {
        // A server that keeps rooms keeps the town from its start.
        if let Some(link) = &mut self.link {
            link.keep(0, &bytes);
        }
        let start = Message::Start {
            players,
            delay,
            check: self.check,
            absent: Vec::new(),
            bytes,
            update: 0,
            epoch: 0,
            speed: 0,
            paused: false,
            tools: Vec::new(),
        };
        if let Some(link) = &mut self.link {
            link.send(None, &start);
        }
        self.start(ctx, start, Vec::new())
    }

    pub(super) fn start(
        &mut self,
        ctx: &mut Ctx,
        start: Message,
        early: Vec<(u8, Message)>,
    ) -> Result<(), String> {
        let link = self.link.take().ok_or("no link")?;
        let mut g = GameplayLoop::from_start(self.player, start, link, early)?;
        g.load(ctx);
        g.startup(ctx);
        g.autosave = None;
        g.online.net.as_mut().unwrap().driven = true;
        self.game = Some(g);
        log::info!("player {} started", self.player);
        Ok(())
    }

    /// Handles what arrived. True when anything did.
    pub fn poll(&mut self, ctx: &mut Ctx) -> Result<bool, String> {
        let Some(g) = &mut self.game else {
            let Some(link) = &mut self.link else {
                return Ok(false);
            };
            let arrived = link.poll();
            let any = !arrived.is_empty();
            let mut arrived = arrived.into_iter();
            while let Some((from, message)) = arrived.next() {
                if matches!(message, Message::Start { .. }) {
                    // What came before the start (a running game's turns), and after it.
                    let mut early = std::mem::take(&mut self.early);
                    early.extend(arrived);
                    self.start(ctx, message, early)?;
                    break;
                }
                self.early.push((from, message));
            }
            return Ok(any);
        };
        let (any, reload) = g.net_poll();
        if let Some((bytes, update)) = reload {
            let old = self.game.take().unwrap();
            self.game = Some(old.reloaded(ctx, &bytes, update)?);
        }
        Ok(any)
    }

    /// Before an update: seals and sends the local turns, and carries the slot out if every
    /// player's turn is here. The updates begun, or none while waiting.
    pub fn begin(&mut self, ctx: &mut Ctx) -> Option<u64> {
        let g = self.game.as_mut()?;
        let ready = g.lockstep_slot(ctx);
        g.net_send();
        ready.then_some(g.sim.updates)
    }

    /// At an update boundary: the host's resync, then the digest checks.
    pub fn boundary(&mut self, ctx: &mut Ctx, update: u64) -> Result<(), String> {
        let g = self.game.as_mut().ok_or("no game")?;
        if let Some((bytes, update)) = g.net_boundary(ctx, update)? {
            let old = self.game.take().unwrap();
            self.game = Some(old.reloaded(ctx, &bytes, update)?);
        }
        Ok(())
    }

    /// The update itself.
    pub fn update(&mut self, ctx: &mut Ctx) {
        let step = unbanished_game::scheduler::seconds(crate::FRAME_STEP);
        if let Some(g) = &mut self.game {
            g.update(ctx, step);
        }
    }

    /// The host's desyncs found and resyncs made.
    pub fn desyncs(&self) -> Vec<u64> {
        self.game
            .as_ref()
            .and_then(|g| g.net())
            .map_or(Vec::new(), |n| n.desyncs.clone())
    }

    pub fn resyncs(&self) -> Vec<u64> {
        self.game
            .as_ref()
            .and_then(|g| g.net())
            .map_or(Vec::new(), |n| n.resyncs.clone())
    }
}

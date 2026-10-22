//! A networked game's turns: polling and sending messages, the host's checks at a turn boundary and resyncs.

use super::*;

impl GameplayLoop {
    /// Handles what arrived: turns, and (the host) the players' digests. A resync's save stops
    /// it and is returned; what came after it waits for the reloaded game. True when anything
    /// arrived.
    pub fn net_poll(&mut self) -> (bool, Option<Reload>) {
        let Some(net) = &mut self.online.net else {
            return (false, None);
        };
        let mut arrived: std::collections::VecDeque<(u8, Message)> = std::mem::take(&mut net.inbox);
        arrived.extend(net.link.poll());
        let left = net.link.left();
        net.leaving.extend(left);
        let any = !arrived.is_empty();
        while let Some((_, message)) = arrived.pop_front() {
            match message {
                Message::Turn(turn) => self.receive_turn(turn),
                Message::Digest {
                    player,
                    epoch,
                    update,
                    crc,
                } => {
                    let l = self.online.lockstep.as_mut().map(|s| &mut s.lockstep);
                    let agree = l.and_then(|l| l.check_in(player, epoch, update, crc));
                    self.online.net.as_mut().unwrap().checked(update, agree);
                }
                // The server found the games different, and this one is to be kept.
                Message::SaveWanted { update } => {
                    let net = self.online.net.as_mut().unwrap();
                    net.resync_wanted = true;
                    log::info!("multiplayer: the server keeps this game from update {update}");
                }
                // The server's check: logged, and a difference noted; the server asks the
                // player whose game is kept for its save.
                Message::Checked { update, agree } => {
                    self.online.net.as_mut().unwrap().noted(update, agree);
                }
                Message::Save { update, bytes } => {
                    self.online.net.as_mut().unwrap().inbox = arrived;
                    return (true, Some((bytes, update)));
                }
                Message::Start { .. } => log::warn!("a start in a game already started"),
                // A player joins the running game: its turns count from slot `from`.
                Message::Grow { player, from } => {
                    if let Some(l) = self.online.lockstep.as_mut() {
                        l.lockstep.add(player, from);
                    }
                    log::info!("multiplayer: player {} joins from slot {from}", player + 1);
                }
            }
        }
        // Who left, once every turn they sent is in.
        let leaving = std::mem::take(&mut self.online.net.as_mut().unwrap().leaving);
        for player in leaving {
            let Some(l) = self.online.lockstep.as_mut().map(|s| &mut s.lockstep) else {
                continue;
            };
            let (from, checks) = l.leave(player);
            log::info!(
                "multiplayer: player {} left; the game goes on from slot {from}",
                player + 1
            );
            let net = self.online.net.as_mut().unwrap();
            for (update, agree) in checks {
                net.checked(update, Some(agree));
            }
        }
        // Who joined the running game: the host brings them in (the others hear it in `Grow`).
        let updates = self.sim.updates;
        let net = self.online.net.as_mut().unwrap();
        let joined = net.link.joined();
        if let Some(l) = self
            .online
            .lockstep
            .as_mut()
            .filter(|l| l.lockstep.player == 0)
        {
            for player in joined {
                // Every turn for slot `from` on is sealed after the newcomer was in the room:
                // a player seals slot S during update S - delay, which it reaches only after
                // the host passed update S - 2 delay - 1. And no player has run slot `from`
                // yet: none is more than `delay` updates ahead of the host.
                let from = updates + 2 * l.lockstep.delay + 2;
                l.lockstep.add(player, from);
                net.link.send(None, &Message::Grow { player, from });
                net.joining.push((player, from - 1));
                log::info!("multiplayer: player {} joins from slot {from}", player + 1);
            }
        }
        (any, None)
    }

    /// Sends the sealed turns.
    pub fn net_send(&mut self) {
        let outbox = self.take_outbox();
        if let Some(net) = &mut self.online.net {
            for message in outbox {
                net.link.send(None, &message);
            }
        }
    }

    /// At an update boundary (once): the host resyncs everybody if a check failed (the save is
    /// sent, and returned for its own game), then every player sends the host its digest when
    /// one is due.
    pub fn net_boundary(&mut self, ctx: &mut Ctx, update: u64) -> Result<Option<Reload>, String> {
        if self.online.net.is_none() || self.online.boundary_done == Some(update) {
            return Ok(None);
        }
        self.online.boundary_done = Some(update);
        let host = self
            .online
            .lockstep
            .as_ref()
            .is_some_and(|l| l.lockstep.player == 0);
        // The host resyncs when its own check failed; any player when the server asked it.
        if self.online.net.as_ref().unwrap().resync_wanted {
            let bytes = self
                .save_file(ctx.archive, ctx.ui, ctx.mods, None, None)
                .ok_or("this player could not save")?;
            let net = self.online.net.as_mut().unwrap();
            net.resync_wanted = false;
            net.resyncs.push(update);
            let save = Message::Save {
                update,
                bytes: bytes.clone(),
            };
            net.link.send(None, &save);
            if host {
                net.link.keep(update, &bytes);
            }
            return Ok(Some((bytes, update)));
        }
        // A player joining the running game gets the save of the update before its first slot,
        // as its start.
        let due: Vec<u8> = {
            let net = self.online.net.as_mut().unwrap();
            let due = net
                .joining
                .iter()
                .filter(|&&(_, at)| at == update)
                .map(|&(p, _)| p)
                .collect();
            net.joining.retain(|&(_, at)| at > update);
            due
        };
        if !due.is_empty() {
            // Everybody goes on from one save: the others reload it (a resync), and the newcomer
            // starts from it. So every town has its templates in the same order.
            let bytes = self
                .save_file(ctx.archive, ctx.ui, ctx.mods, None, None)
                .ok_or("the host could not save for a joining player")?;
            let l = &self.online.lockstep.as_ref().ok_or("no lockstep")?.lockstep;
            let start = Message::Start {
                players: l.players,
                delay: l.delay,
                check: self.online.net.as_ref().unwrap().check,
                absent: l.absent(),
                bytes: bytes.clone(),
                update,
                epoch: l.epoch + 1,
                speed: self.speed,
                paused: self.paused,
                tools: self.sim.drag_tools.keys().cloned().collect(),
            };
            let net = self.online.net.as_mut().unwrap();
            let save = Message::Save {
                update,
                bytes: bytes.clone(),
            };
            net.link.send(None, &save);
            for player in due {
                net.link.send(Some(player), &start);
                log::info!(
                    "multiplayer: player {} starts at update {update}",
                    player + 1
                );
            }
            net.link.keep(update, &bytes);
            return Ok(Some((bytes, update)));
        }
        // A server that keeps rooms gets the host's town every minute of play.
        if host && update > 0 && update.is_multiple_of(KEEP_EVERY) {
            let keeps = self.online.net.as_ref().is_some_and(|n| n.keeps);
            if keeps {
                let bytes = self
                    .save_file(ctx.archive, ctx.ui, ctx.mods, None, None)
                    .ok_or("the host could not save its town to keep")?;
                let net = self.online.net.as_mut().unwrap();
                net.keeps = net.link.keep(update, &bytes);
            }
        }
        let check = self.online.net.as_ref().unwrap().check;
        if check > 0 && update > 0 && update.is_multiple_of(check) {
            let crc = self.digest(ctx, update as u32)?.crc;
            let l = &mut self.online.lockstep.as_mut().ok_or("no lockstep")?.lockstep;
            let (player, epoch) = (l.player, l.epoch);
            // A server that checks the games gets every player's digest, the host's too.
            if self
                .online
                .net
                .as_mut()
                .unwrap()
                .link
                .send_digest(epoch, update, crc)
            {
                return Ok(None);
            }
            let l = &mut self.online.lockstep.as_mut().ok_or("no lockstep")?.lockstep;
            let agree = (player == 0).then(|| l.check(0, update, crc)).flatten();
            let net = self.online.net.as_mut().unwrap();
            if player == 0 {
                net.checked(update, agree);
            } else {
                let message = Message::Digest {
                    player,
                    epoch,
                    update,
                    crc,
                };
                net.link.send(Some(0), &message);
            }
        }
        Ok(None)
    }

    /// The host found a desync it has not resynced yet.
    pub fn resync_pending(&self) -> bool {
        self.online.net.as_ref().is_some_and(|n| n.resync_wanted)
    }
}

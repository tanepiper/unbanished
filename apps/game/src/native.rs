//! The commands that run without a window (native only): headless captures and the probe
//! (`--boot-capture`), map pictures (`--capture`), `--sim-digest`, `--replay`, and the lockstep
//! and network tests.

use super::*;

pub fn data_dir(args: &[String]) -> std::path::PathBuf {
    crate::platform::paths::data_dir(args)
}

pub fn read_packages(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"]
        .iter()
        .filter_map(|name| match std::fs::read(dir.join(name)) {
            Ok(bytes) => Some((name.to_string(), bytes)),
            Err(e) => {
                log::warn!("{}: {e}", dir.join(name).display());
                None
            }
        })
        .collect()
}

pub fn write_png(path: &str, (w, h): (u32, u32), rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .expect("write png");
    log::info!("wrote {path}");
}

/// Run the boot sequence headlessly at 30 frames per second and capture one frame.
pub fn boot_capture(args: &[String], out: &str, seconds: f32) {
    // Captures ignore the player's stored settings; `--ui-scale PERCENT` sets UIScale.
    let mut settings = crate::settings::Settings::default();
    if let Some(v) = args
        .windows(2)
        .find(|w| w[0] == "--ui-scale")
        .and_then(|w| w[1].parse().ok())
    {
        settings.game.ui_scale = v;
    }
    // `--setting KEY=VALUE` sets a stored setting by its original name (`InputMouseDisplay=1`,
    // `VideoReflectionDetail=2`, `InputForward=297`, …).
    let stored: Vec<&str> = args
        .windows(2)
        .filter(|w| w[0] == "--setting")
        .map(|w| w[1].as_str())
        .collect();
    if !stored.is_empty() {
        settings.persistence.parse(&stored.join("\n"));
        let p = &settings.persistence;
        settings.video = crate::settings::VideoConfig::load(p);
        settings.audio = crate::settings::AudioConfig::load(p);
        settings.input = crate::settings::InputConfig::load(p);
        settings.game = crate::settings::GameOptions::load(p);
        settings.keymap = crate::keymap::KeyMap::load(p);
    }
    let mut game = {
        let dir = data_dir(args);
        let found = crate::mods::find(&dir);
        Game::with_mods(read_packages(&dir), found, Some(dir), settings).expect("game data")
    };
    game.audio.set_master_volume(0.0);
    // `--capture-size WxH` matches a reference frame's resolution (default 1280x800).
    let size = args
        .windows(2)
        .find(|w| w[0] == "--capture-size")
        .and_then(|w| {
            let (a, b) = w[1].split_once('x')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        })
        .unwrap_or((1280, 800));
    let mut renderer = pollster::block_on(Renderer::headless_with(
        size.0,
        size.1,
        gfx::render::VideoSettings::from(&game.settings.video),
    ));
    renderer.blocking_readback = true;
    // `--scene-scale PART` draws the 3D scene at that part of the frame's size (the browser's
    // resolution setting), to check it without a browser.
    if let Some(part) = args
        .windows(2)
        .find(|w| w[0] == "--scene-scale")
        .and_then(|w| w[1].parse::<f32>().ok())
    {
        renderer.set_scene_scale(part);
    }
    // The Options dialog lists the capture device with one mode, the capture size.
    game.settings.display = renderer.display.clone();
    for adapter in game
        .settings
        .display
        .interfaces
        .iter_mut()
        .flat_map(|i| i.adapters.iter_mut())
    {
        adapter.modes = vec![settings::DisplayMode {
            width: size.0 as i32,
            height: size.1 as i32,
            refresh: vec![(60, 1)],
        }];
    }
    // `--press NAME@SECONDS` clicks a control at a given time.
    let mut presses: Vec<(String, f32)> = args
        .windows(2)
        .filter(|w| w[0] == "--press")
        .filter_map(|w| {
            let (name, at) = w[1].split_once('@')?;
            Some((name.to_string(), at.parse().ok()?))
        })
        .collect();
    // `--text NAME=TEXT@SECONDS` clicks the named edit, empties it and types TEXT.
    let mut texts: Vec<(String, String, f32)> = args
        .windows(2)
        .filter(|w| w[0] == "--text")
        .filter_map(|w| {
            let (what, at) = w[1].rsplit_once('@')?;
            let (name, text) = what.split_once('=')?;
            Some((name.to_string(), text.to_string(), at.parse().ok()?))
        })
        .collect();
    // `--move X,Y@SECONDS` moves the cursor; `--click-at X,Y@SECONDS` also presses and
    // releases the left button there; `--mouse-down` and `--mouse-up` move there and press
    // or release it, for drags (with `--move` between).
    let mut pointer: Vec<(&str, i32, i32, f32)> = args
        .windows(2)
        .filter(|w| {
            matches!(
                w[0].as_str(),
                "--move" | "--click-at" | "--mouse-down" | "--mouse-up"
            )
        })
        .filter_map(|w| {
            let (xy, at) = w[1].split_once('@')?;
            let (x, y) = xy.split_once(',')?;
            Some((
                w[0].as_str(),
                x.parse().ok()?,
                y.parse().ok()?,
                at.parse().ok()?,
            ))
        })
        .collect();
    // `--wheel STEPS@SECONDS` scrolls the camera.
    let mut wheels: Vec<(f32, f32)> = args
        .windows(2)
        .filter(|w| w[0] == "--wheel")
        .filter_map(|w| {
            let (steps, at) = w[1].split_once('@')?;
            Some((steps.parse().ok()?, at.parse().ok()?))
        })
        .collect();
    // `--hold ACTION@START-END` holds the default key of a key-map action (`RotateLeft`,
    // `Forward`, `PitchUp`, …) from START to END seconds: camera moves in captures.
    let mut holds: Vec<(u8, f32, f32, bool)> = args
        .windows(2)
        .filter(|w| w[0] == "--hold")
        .filter_map(|w| {
            let (name, span) = w[1].split_once('@')?;
            let (start, end) = span.split_once('-')?;
            let code = keymap::default_key(name);
            if code.is_none() {
                log::warn!("--hold {name}: no such keyboard action");
            }
            Some((code?, start.parse().ok()?, end.parse().ok()?, false))
        })
        .collect();
    // `--combo NAME=ITEM@SECONDS` selects a combo item by its text (New Game settings).
    let mut combos: Vec<(String, String, f32)> = args
        .windows(2)
        .filter(|w| w[0] == "--combo")
        .filter_map(|w| {
            let (sel, at) = w[1].rsplit_once('@')?;
            let (name, item) = sel.split_once('=')?;
            Some((name.to_string(), item.to_string(), at.parse().ok()?))
        })
        .collect();
    // `--event KIND[:CELL]@SECONDS` posts an event (event ribbon and log captures);
    // `--script "COMMAND@SECONDS"` runs a loop script command (`open TEMPLATE`).
    let mut events: Vec<(String, f32)> = args
        .windows(2)
        .filter_map(|w| {
            let (cmd, at) = w[1].rsplit_once('@')?;
            let cmd = match w[0].as_str() {
                "--event" => format!("event {}", cmd.replace(':', " ")),
                "--script" => cmd.to_string(),
                _ => return None,
            };
            Some((cmd, at.parse().ok()?))
        })
        .collect();
    // `--probe "COMMAND@SECONDS"` runs a probe command ([`probe`]) and writes its answer as a
    // JSON line to the log, and to `--probe-out FILE` when given.
    #[cfg(feature = "probe")]
    let mut probes: Vec<(String, f32)> = args
        .windows(2)
        .filter(|w| w[0] == "--probe")
        .filter_map(|w| {
            let (cmd, at) = w[1].rsplit_once('@')?;
            Some((cmd.to_string(), at.parse().ok()?))
        })
        .collect();
    #[cfg(feature = "probe")]
    let mut probe_out = args
        .windows(2)
        .find(|w| w[0] == "--probe-out")
        .and_then(|w| std::fs::File::create(&w[1]).ok());
    // `--camera`, `--camera-follow` and their settings: the cinematic camera (drawing only).
    #[cfg(feature = "cinematic")]
    let mut cinematic =
        crate::view::cinematic::Cinematic::from_args(args).filter(|_| !platform::mode::pure());
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    // `--realtime`: frames keep to the clock (a capture that talks to a relay).
    let realtime = args
        .iter()
        .any(|a| a == "--realtime")
        .then(std::time::Instant::now);
    // Frames left to draw for a scripted screenshot's readback.
    let mut shot_frames = 0;
    while t < seconds {
        if let Some(start) = realtime {
            let ahead = std::time::Duration::from_secs_f32(t).saturating_sub(start.elapsed());
            std::thread::sleep(ahead);
        }
        // A 1/30 s frame is two of `MainLoop::Frame`'s 1/60 s steps.
        let step = unbanished_game::scheduler::seconds(FRAME_STEP);
        platform::perf::frame_begin();
        for _ in 0..2 {
            let _p = platform::perf::scope("update");
            game.update(step);
        }
        t += dt;
        #[cfg(feature = "cinematic")]
        if let Some(c) = cinematic.as_mut() {
            game.cinematic = c.step(t, dt, game.main.current(), &game.archive);
        }
        // A save waits for a drawn frame's preview (headless, nothing else is drawn). With
        // the profiler on, every frame is drawn, as in the windowed game.
        if game.main.current().wants_save_preview() || shot_frames > 0 || platform::perf::enabled()
        {
            let _p = platform::perf::scope("draw");
            game.draw(&mut renderer, false);
            shot_frames -= (shot_frames > 0) as i32;
        }
        platform::perf::frame_end();
        combos.retain(|(name, item, at)| {
            if t < *at {
                return true;
            }
            if !game.select_combo(name, item, size) {
                log::warn!("--combo {name}={item}: not found at {t:.2} s");
            }
            false
        });
        events.retain(|(command, at)| {
            if t < *at {
                return true;
            }
            if !game.script(command) {
                log::warn!("--event {command}: not in a game at {t:.2} s");
            }
            if command.starts_with("screenshot") {
                shot_frames = 5;
            }
            false
        });
        #[cfg(feature = "probe")]
        probes.retain(|(command, at)| {
            if t < *at {
                return true;
            }
            let t = (f64::from(*at) * 1000.0).round() / 1000.0;
            let answer = match probe::run(&mut game, size, command) {
                Ok(v) => serde_json::json!({ "t": t, "command": command, "ok": v }),
                Err(e) => serde_json::json!({ "t": t, "command": command, "error": e }),
            };
            log::info!("probe {answer}");
            if let Some(f) = probe_out.as_mut() {
                use std::io::Write;
                let _ = writeln!(f, "{answer}");
            }
            false
        });
        wheels.retain(|&(steps, at)| {
            if t < at {
                return true;
            }
            game.wheel(steps);
            false
        });
        holds.retain_mut(|(code, start, end, down)| {
            if !*down && t >= *start {
                game.key(None, None, Some(*code));
                *down = true;
            }
            if *down && t >= *end {
                game.released((keymap::KEYBOARD, *code));
                return false;
            }
            true
        });
        pointer.retain(|&(kind, x, y, at)| {
            if t < at {
                return true;
            }
            let logical = game.logical(size);
            if let Some(screen) = game.main.current().screen() {
                screen.layout(&mut game.ui, logical);
            }
            game.pointer_move(x, y);
            match kind {
                "--click-at" => {
                    game.pointer_button(true);
                    game.pointer_button(false);
                }
                "--mouse-down" => {
                    game.pointer_button(true);
                }
                "--mouse-up" => {
                    game.pointer_button(false);
                }
                _ => {}
            }
            false
        });
        presses.retain(|(name, at)| {
            if t < *at {
                return true;
            }
            if !game.press(name, size) {
                log::warn!("--press {name}: not found at {t:.2} s");
            }
            false
        });
        texts.retain(|(name, text, at)| {
            if t < *at {
                return true;
            }
            if !game.type_into(name, text, size) {
                log::warn!("--text {name}: not typed at {t:.2} s");
            }
            false
        });
    }
    let rgba = game.draw(&mut renderer, true).expect("capture");
    write_png(out, size, &rgba);
}

/// `--sim-digest SAVE [UPDATES] [EVERY]`: runs a save without a window at speed 10 and
/// prints a digest of the game state every EVERY updates (`gameplay::digest`), to compare
/// with the browser build's `?digest=`.
pub fn sim_digest(args: &[String]) {
    let save = args.get(1).expect("--sim-digest SAVE [UPDATES] [EVERY]");
    let updates = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1200);
    let every = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);
    let bytes = std::fs::read(save).expect("read the save");
    let dir = data_dir(args);
    let found = crate::mods::find(&dir);
    let mut game = Game::with_mods(
        read_packages(&dir),
        found,
        Some(dir),
        crate::settings::Settings::default(),
    )
    .expect("game data");
    game.audio.set_master_volume(0.0);
    let start = std::time::Instant::now();
    match crate::gameplay::digest::run(&mut game, &bytes, updates, every) {
        Ok(digests) => {
            for d in digests {
                println!("{d}");
            }
            log::info!(
                "{updates} updates in {:.1} s",
                start.elapsed().as_secs_f32()
            );
        }
        Err(e) => {
            eprintln!("--sim-digest: {e}");
            std::process::exit(1);
        }
    }
}

/// `--replay SAVE RECORDING [UPDATES] [EVERY]`: replays a recording (`--record`) from its
/// save without a window and compares the game state with the recorded digests
/// (`gameplay::recording`). Exits with 1 when they differ.
#[cfg(feature = "replay")]
pub fn replay(args: &[String]) {
    let (Some(save), Some(recording)) = (args.get(1), args.get(2)) else {
        eprintln!("--replay SAVE RECORDING [UPDATES] [EVERY]");
        std::process::exit(2);
    };
    let updates = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let every = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let bytes = std::fs::read(save).expect("read the save");
    let text = std::fs::read_to_string(recording).expect("read the recording");
    let dir = data_dir(args);
    let found = crate::mods::find(&dir);
    let mut game = Game::with_mods(
        read_packages(&dir),
        found,
        Some(dir),
        crate::settings::Settings::default(),
    )
    .expect("game data");
    game.audio.set_master_volume(0.0);
    let start = std::time::Instant::now();
    match crate::gameplay::recording::replay(&mut game, &bytes, &text, updates, every) {
        Ok(r) => {
            for d in &r.digests {
                println!("{d}");
            }
            log::info!("replayed in {:.1} s", start.elapsed().as_secs_f32());
            match r.mismatch {
                Some((recorded, replayed)) => {
                    println!("DIFFERENT from the recording");
                    println!("  recorded {recorded}");
                    println!("  replayed {replayed}");
                    std::process::exit(1);
                }
                None => println!("identical: {} recorded digests matched", r.compared),
            }
        }
        Err(e) => {
            eprintln!("--replay: {e}");
            std::process::exit(2);
        }
    }
}

/// `--lockstep-test SAVE RECORDING [PLAYERS] [DELAY] [JITTER] [STALL] [SEED] [CHECK]
/// [CORRUPT]`: plays a recording's commands, split between PLAYERS (default 3), in lockstep
/// games joined by a simulated network (`gameplay::lockstep::simulate`), and compares their
/// digests. The host checks digests every CHECK updates (default 600, 0 none) and resyncs
/// the players from its save when they differ; CORRUPT `PLAYER:UPDATE` scrambles one
/// player's game there, to test that. Exits with 1 when the players disagree at the end.
#[cfg(feature = "multiplayer")]
pub fn lockstep_test(args: &[String]) {
    let (Some(save), Some(recording)) = (args.get(1), args.get(2)) else {
        eprintln!(
            "--lockstep-test SAVE RECORDING [PLAYERS] [DELAY] [JITTER] [STALL] [SEED] [CHECK] [CORRUPT]"
        );
        std::process::exit(2);
    };
    let number =
        |i: usize, default: u64| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(default);
    let sim = crate::gameplay::network::Simulation {
        players: number(3, 3) as u8,
        delay: number(4, 6),
        jitter: number(5, 8),
        stall: number(6, 20),
        every: 60,
        check: number(8, 600),
        corrupt: args.get(9).and_then(|s| {
            let (p, u) = s.split_once(':')?;
            Some((p.parse().ok()?, u.parse().ok()?))
        }),
        seed: number(7, 1),
    };
    let bytes = std::fs::read(save).expect("read the save");
    let text = std::fs::read_to_string(recording).expect("read the recording");
    let dir = data_dir(args);
    let found = crate::mods::find(&dir);
    let mut game = Game::with_mods(
        read_packages(&dir),
        found,
        Some(dir),
        crate::settings::Settings::default(),
    )
    .expect("game data");
    game.audio.set_master_volume(0.0);
    let start = std::time::Instant::now();
    match crate::gameplay::network::simulate(&mut game, &bytes, &text, sim, 0) {
        Ok(r) => {
            for d in &r.digests[0] {
                println!("{d}");
            }
            log::info!("simulated in {:.1} s", start.elapsed().as_secs_f32());
            println!(
                "{sim:?}: {} ticks, players at most {} updates apart",
                r.ticks, r.lead
            );
            println!(
                "desyncs found at updates {:?}, resyncs at {:?}",
                r.desyncs, r.resyncs
            );
            if r.recorded > 0 {
                match r.recorded_mismatch {
                    Some((a, b)) => println!("DIFFERENT from the recording: {a} / {b}"),
                    None => println!("{} recorded digests matched", r.recorded),
                }
            }
            match r.mismatch {
                Some((p, a, b)) => {
                    println!("PLAYERS DISAGREE: player {p}");
                    println!("  player 0 {a}");
                    println!("  player {p} {b}");
                    std::process::exit(1);
                }
                None if r.recorded_mismatch.is_some() => std::process::exit(1),
                None => println!(
                    "identical: {} players agree on {} digests",
                    sim.players,
                    r.digests[0].len()
                ),
            }
        }
        Err(e) => {
            eprintln!("--lockstep-test: {e}");
            std::process::exit(2);
        }
    }
}

/// `--net-test RELAY host|join ROOM SAVE RECORDING PLAYERS [DELAY] [CHECK] [CORRUPT]`: one
/// player of a networked lockstep game through a relay (`unbanished-server`). The host opens
/// ROOM, waits for PLAYERS, and starts the game from SAVE (joining players get it from the
/// host and ignore SAVE); each plays its share of RECORDING's commands and prints its
/// digests. CORRUPT `UPDATE` scrambles this player's game there. tools/compat/network.sh runs
/// several and compares them.
#[cfg(feature = "multiplayer")]
pub fn net_test(args: &[String]) {
    use crate::gameplay::network::{NetPeer, RelayLink, Script, Shared};
    let usage = "--net-test RELAY host|join ROOM SAVE RECORDING PLAYERS [DELAY] [CHECK] [CORRUPT]";
    let (Some(relay), Some(role), Some(room), Some(save), Some(recording), Some(players)) = (
        args.get(1),
        args.get(2),
        args.get(3),
        args.get(4),
        args.get(5),
        args.get(6).and_then(|s| s.parse::<u8>().ok()),
    ) else {
        eprintln!("{usage}");
        std::process::exit(2);
    };
    let number =
        |i: usize, default: u64| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (delay, check) = (number(7, 6), number(8, 120));
    let corrupt = args.get(9).and_then(|s| s.parse::<u64>().ok());
    // `--leave-at N`: this player leaves the game at update N. `--resume`: the host goes on
    // from the room's town kept by the server, if it has one, instead of SAVE.
    let leave_at = args
        .windows(2)
        .find(|w| w[0] == "--leave-at")
        .and_then(|w| w[1].parse::<u64>().ok());
    let resume = args.iter().any(|a| a == "--resume");
    let host = role == "host";
    let text = std::fs::read_to_string(recording).expect("read the recording");
    let dir = data_dir(args);
    let found = crate::mods::find(&dir);
    let mut game = Game::with_mods(
        read_packages(&dir),
        found,
        Some(dir),
        crate::settings::Settings::default(),
    )
    .expect("game data");
    game.audio.set_master_volume(0.0);
    let link = std::rc::Rc::new(std::cell::RefCell::new(RelayLink::new(
        unbanished_net::Connection::connect(relay),
        crate::gameplay::network::hello(&game.archive),
    )));
    let request = match host {
        true => unbanished_net::ToRelay::Host {
            room: Some(room.clone()),
            password: None,
        },
        false => unbanished_net::ToRelay::Join {
            room: room.clone(),
            password: None,
        },
    };
    link.borrow_mut().request(request);
    // Who this player is, and (the host) everybody there.
    let wait = |what: &str, done: &mut dyn FnMut(&unbanished_net::FromRelay) -> bool| {
        let start = std::time::Instant::now();
        loop {
            let _ = crate::gameplay::network::Link::poll(&mut *link.borrow_mut());
            let events: Vec<_> = link.borrow_mut().events.drain(..).collect();
            for e in &events {
                if let unbanished_net::FromRelay::Error { message } = e {
                    eprintln!("relay: {message}");
                    std::process::exit(2);
                }
                if done(e) {
                    return;
                }
            }
            if let unbanished_net::State::Closed(e) = link.borrow().state() {
                eprintln!("relay: {e}");
                std::process::exit(2);
            }
            if start.elapsed().as_secs() > 120 {
                eprintln!("waited too long for {what}");
                std::process::exit(2);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    let mut me = 0u8;
    let mut kept = None;
    wait("the room", &mut |e| match e {
        unbanished_net::FromRelay::Hosted { kept: k, .. } => {
            kept = *k;
            true
        }
        unbanished_net::FromRelay::Joined { player, .. } => {
            me = *player;
            true
        }
        _ => false,
    });
    log::info!("player {me} in room {room}");
    let mut peer = NetPeer::new(me, Box::new(Shared(link.clone())), check);
    let mut script = Script::new(&text, players as usize, delay, false).expect("the recording");
    let last = script.last + delay + 1;
    let start = std::time::Instant::now();
    let outcome = game.with_ctx(|_, ctx| -> Result<Vec<String>, String> {
        if host {
            let mut joined = 1;
            wait("the players", &mut |e| {
                if let unbanished_net::FromRelay::PeerJoined { .. } = e {
                    joined += 1;
                }
                joined >= players
            });
            let bytes = match kept.filter(|_| resume) {
                Some(update) => {
                    link.borrow_mut().request(unbanished_net::ToRelay::Fetch);
                    let mut town = Vec::new();
                    wait("the kept town", &mut |e| match e {
                        unbanished_net::FromRelay::Kept { bytes, .. } => {
                            town = bytes.clone();
                            true
                        }
                        _ => false,
                    });
                    log::info!("resuming the room's town kept at update {update}");
                    town
                }
                None => std::fs::read(save).map_err(|e| e.to_string())?,
            };
            peer.host_start(ctx, bytes, players, delay)?;
        }
        let mut lines = Vec::new();
        let mut corrupt = corrupt;
        loop {
            let arrived = peer.poll(ctx)?;
            let Some(g) = peer.game.as_mut() else {
                if !arrived {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                continue;
            };
            if g.updates() > last {
                break;
            }
            script.give(me as usize, g, ctx);
            let Some(update) = peer.begin(ctx) else {
                if !arrived {
                    std::thread::sleep(std::time::Duration::from_micros(500));
                }
                continue;
            };
            if leave_at.is_some_and(|at| update >= at) {
                log::info!("player {me} leaves at update {update}");
                break;
            }
            if corrupt.is_some_and(|at| update >= at) {
                peer.game.as_mut().unwrap().scramble();
                corrupt = None;
            }
            peer.boundary(ctx, update)?;
            if update % 60 == 0 {
                // Printed at once, so that a run cut short can be compared too; after a
                // resync an update can come again, and its last line counts.
                let d = peer.game.as_ref().unwrap().state_digest(ctx, update)?;
                println!("{d}");
                lines.push(d.to_string());
            }
            peer.update(ctx);
        }
        // Let the last messages go out before the connection closes.
        let linger = std::time::Instant::now();
        while linger.elapsed().as_millis() < 1500 {
            peer.poll(ctx)?;
            peer.begin(ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(lines)
    });
    match outcome {
        Ok(_) => {
            println!("desyncs {:?} resyncs {:?}", peer.desyncs(), peer.resyncs());
            log::info!(
                "player {me}: {} updates in {:.1} s",
                last,
                start.elapsed().as_secs_f32()
            );
        }
        Err(e) => {
            eprintln!("--net-test: {e}");
            std::process::exit(2);
        }
    }
}

/// The generated-map capture from M5 (`--capture`).
pub fn map_capture(args: &[String], out: &str) {
    use crate::view::scene::{Camera, SceneCamera, SceneView};
    use crate::view::world::{World, WorldParams};
    let mut params = WorldParams::default();
    let (mut month, mut overview) = (3usize, false);
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if let Some(v) = value("--seed") {
        params.seed = v.parse().expect("--seed N");
    }
    if let Some(v) = value("--terrain") {
        params.terrain = v;
    }
    if let Some(v) = value("--size") {
        params.size = v.parse().expect("--size 0|1|2");
    }
    if let Some(v) = value("--month") {
        month = v.parse::<usize>().expect("--month 0..11") % 12;
    }
    if let Some(v) = value("--view") {
        overview = v == "overview";
    }
    let mut archive = Archive::default();
    for (_, bytes) in read_packages(&data_dir(args)) {
        archive.add(Package::parse(bytes).expect("parse package"));
    }
    let world = World::generate(&archive, &params).expect("generate world");
    let mut lighting = gfx::render::Lighting::from_month(&world.environment.months[month]);
    let camera = if overview {
        // Fog distances are tuned for the in-game camera; push them out for whole-map views.
        lighting.fog_start = 1e5;
        lighting.fog_end = 2e5;
        Camera::overview(&world.terrain)
    } else {
        Camera::ground()
    };
    let size = (1280, 800);
    let mut renderer = pollster::block_on(Renderer::headless(size.0, size.1));
    let view = SceneView::new(world.terrain, SceneCamera::Map(camera), lighting);
    renderer.set_scene(&view);
    let rgba = renderer.capture(&Frame {
        terrain: Some(view.frame(renderer.aspect(), 0.0)),
        clear: wgpu::Color {
            r: 0.75,
            g: 0.75,
            b: 0.75,
            a: 1.0,
        },
        ui: None,
    });
    write_png(out, size, &rgba);
}

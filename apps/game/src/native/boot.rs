//! The headless boot capture: start the game, press buttons and take screenshots.

use super::*;

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
        crate::view::cinematic::Cinematic::from_args(args).filter(|_| !platform::mode::vanilla());
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

//! Un-Banished's launcher: GameFrame's frame in front of the game (docs/develop/FRAME.md). The
//! player chooses the game (Banished exactly, or Un-Banished with its extensions) and their
//! copy of Banished's files (found in Steam, or a folder they choose); the launcher starts
//! `unbanished --frame-io` as the game's kernel, one program per game.
//!
//! ```text
//! unbanished-frame [--unbanished PATH] [--data DIR] [--frame | --no-frame] [--preset NAME]
//!                  [-- UNBANISHED-OPTIONS...]
//! ```
//!
//! - `--unbanished PATH`: the game's program. The default is `unbanished` beside this program.
//! - `--data DIR`: where the launcher keeps its settings (`settings.json`). The default is the
//!   current folder, which `gameframe install` makes the install's data folder.
//! - `--frame` opens the window even when the player turned it off at start; `--no-frame`
//!   starts the game at once with the window hidden.
//! - `--preset NAME`: a ready-made theme until the player picks one in Settings (`none` for the
//!   frame's own look).
//! - Options after `--` go to the game on every start.

use std::path::PathBuf;
use std::sync::OnceLock;

use dioxus::desktop::{Config, WindowBuilder};
use dioxus::prelude::*;
use gameframe_frame::{settings_file, Respawn, Session, Settings};
use gameframe_frame_ui::{Frame, FrameSetup};

/// The frame's description: its look, links and help page. Its one section, the game data the
/// spec requires, is not kept: the frame has no library, and the game reads Banished's files
/// where they are (`unbanished`'s `platform::steam`).
const LIBRARY: &str = include_str!("../library.json");
const HELP: &str = include_str!("../help.md");
const LOGO: &str = include_str!("../../../docs/assets/un-banished-icon.svg");

const USAGE: &str =
    "usage: unbanished-frame [--unbanished PATH] [--data DIR] [--frame | --no-frame]
                        [--preset NAME] [-- UNBANISHED-OPTIONS...]";

#[derive(Debug)]
struct Options {
    unbanished: PathBuf,
    data: PathBuf,
    preset: Option<String>,
    /// For the frame's own `--frame` and `--no-frame`.
    args: Vec<String>,
    /// For the game, after `--`.
    game_args: Vec<String>,
}

static OPTIONS: OnceLock<Options> = OnceLock::new();

fn options(args: Vec<String>) -> Result<Options, String> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(program_name())))
        .unwrap_or_else(|| PathBuf::from(program_name()));
    let mut o = Options {
        unbanished: beside,
        data: PathBuf::from("."),
        preset: None,
        args: Vec::new(),
        game_args: Vec::new(),
    };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "--unbanished" => o.unbanished = value("--unbanished")?.into(),
            "--data" => o.data = value("--data")?.into(),
            "--preset" => o.preset = Some(value("--preset")?),
            "--frame" | "--launcher" | "--no-frame" => o.args.push(a),
            "--" => {
                o.game_args = it.collect();
                break;
            }
            s => return Err(format!("unknown option {s}")),
        }
    }
    Ok(o)
}

fn program_name() -> &'static str {
    if cfg!(windows) {
        "unbanished.exe"
    } else {
        "unbanished"
    }
}

fn options_now() -> &'static Options {
    OPTIONS.get().expect("options are read first")
}

fn settings_path() -> PathBuf {
    options_now().data.join("settings.json")
}

/// The spec, with its help page's text and the logo.
fn spec() -> Result<gameframe_library::Spec, String> {
    let mut spec = gameframe_library::Spec::from_json(LIBRARY)?;
    for p in &mut spec.pages {
        if p.src.as_deref() == Some("help.md") {
            p.text = Some(HELP.to_string());
        }
    }
    spec.logo = Some(svg_data_url(LOGO));
    Ok(spec)
}

/// An SVG as a `data:` address an `<img>` can show.
fn svg_data_url(svg: &str) -> String {
    let mut s = String::from("data:image/svg+xml;utf8,");
    for c in svg.trim().chars() {
        match c {
            '#' => s.push_str("%23"),
            '%' => s.push_str("%25"),
            '\n' | '\r' => s.push(' '),
            c => s.push(c),
        }
    }
    s
}

/// The game's arguments for one start.
fn game_args() -> Vec<String> {
    let mut args = vec!["--frame-io".to_string()];
    args.extend(options_now().game_args.iter().cloned());
    args
}

fn main() {
    let options = match options(std::env::args().skip(1).collect()) {
        Ok(o) => o,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("unbanished-frame: {e}");
            }
            eprintln!("{USAGE}");
            std::process::exit(if e.is_empty() { 0 } else { 2 });
        }
    };
    let _ = std::fs::create_dir_all(&options.data);
    let shown =
        settings_file::load(&options.data.join("settings.json")).show_at_start(&options.args);
    let _ = OPTIONS.set(options);
    let window = WindowBuilder::new()
        .with_title("Un-Banished")
        .with_visible(shown);
    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window).with_menu(None))
        .launch(app);
}

fn app() -> Element {
    match use_hook(make_setup) {
        Ok(setup) => rsx! { Frame { setup } },
        Err(e) => rsx! { p { "Un-Banished's launcher could not start: {e}" } },
    }
}

fn make_setup() -> Result<FrameSetup, String> {
    let o = options_now();
    let mut spec = spec()?;
    let program = o.unbanished.to_string_lossy().into_owned();
    // One program per game: the game's window can be made only once in a process (winit).
    let game = Respawn::spawn(&program, Box::new(game_args))
        .map_err(|e| format!("{e} (the game's program: {program})"))?;
    let settings = settings_file::load(&settings_path());
    let shown = settings.show_at_start(&o.args);
    let mut session = Session::new(Box::new(game), settings)?;
    session.hide_while_running(true);
    if let Some(p) = &o.preset {
        spec.preset = (p != "none").then(|| p.clone());
    }
    dioxus::desktop::window().set_title(&spec.title);
    Ok(FrameSetup::new(session, spec)
        .start_at_once(!shown)
        .with_window(|shown| {
            let window = dioxus::desktop::window();
            window.set_visible(shown);
            if shown {
                window.set_focus();
            }
        })
        .with_exit(|| std::process::exit(0))
        .with_save(|s: &Settings| {
            if let Err(e) = settings_file::save(&settings_path(), s) {
                eprintln!("unbanished-frame: could not save the settings: {e}");
            }
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spec_reads_with_its_help_and_logo() {
        let s = spec().unwrap();
        assert!(s.pages[0].text.as_deref().unwrap().contains("Un-Banished"));
        assert_eq!(s.sections.len(), 1);
        let logo = s.logo.unwrap();
        assert!(logo.starts_with("data:image/svg+xml;utf8,<svg"));
        assert!(!logo.contains('#'));
    }

    #[test]
    fn options_after_the_marker_go_to_the_game() {
        let args = ["--data", "d", "--no-frame", "--", "--seed", "3"].map(String::from);
        let o = options(args.to_vec()).unwrap();
        assert_eq!(o.data, PathBuf::from("d"));
        assert_eq!(o.args, ["--no-frame"]);
        assert_eq!(o.game_args, ["--seed", "3"]);
        assert!(options(vec!["--what".into()]).is_err());
    }
}

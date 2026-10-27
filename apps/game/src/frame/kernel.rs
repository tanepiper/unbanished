//! The frame's side of a start: the settings it sends, turned into a [`Launch`]. It draws
//! nothing and opens no window, so a test drives it with GameFrame's `Session`.

use std::path::PathBuf;

use gameframe_frame::{topics, Choice, Event, Game, Manifest, Outbox};
use serde_json::{json, Map, Value};

use super::manifest::{
    self, ARGS, BANISHED, FIRST_FOUND, FOLDER, FOLDER_PATH, INSTALL, MACHINE, MUTE,
};
use crate::platform::steam;

/// One start of the game, as the frame asked for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Launch {
    /// The `WinData` folder to read.
    pub data: PathBuf,
    /// Banished exactly: every extension off (`--vanilla`).
    pub vanilla: bool,
    pub mute: bool,
    /// More of the program's own options.
    pub args: Vec<String>,
}

pub(crate) struct Kernel {
    settings: Map<String, Value>,
    /// The copies of Banished found on this computer.
    installs: Vec<steam::Install>,
    launch: Option<Launch>,
}

impl Kernel {
    pub(crate) fn new(installs: Vec<steam::Install>) -> Kernel {
        Kernel {
            settings: Map::new(),
            installs,
            launch: None,
        }
    }

    /// The launch `game.start` asked for, once.
    pub(crate) fn take_launch(&mut self) -> Option<Launch> {
        self.launch.take()
    }

    /// `config.options` for [`INSTALL`]: the first copy found, each copy found, then the
    /// player's own folder.
    fn offer_installs(&self, out: &mut Outbox) {
        let first = match self.installs.first() {
            Some(i) => format!("The first copy found: {}", i.label),
            None => "The first copy found (none found in Steam)".into(),
        };
        let mut options = vec![Choice {
            value: FIRST_FOUND.into(),
            label: first,
        }];
        // The label names the place; two copies in one place are told apart by number.
        options.extend(self.installs.iter().enumerate().map(|(n, i)| {
            let same = self.installs.iter().filter(|j| j.label == i.label).count();
            Choice {
                value: i.win_data.to_string_lossy().into_owned(),
                label: if same > 1 {
                    format!("{} ({})", i.label, n + 1)
                } else {
                    i.label.clone()
                },
            }
        }));
        options.push(Choice {
            value: FOLDER.into(),
            label: "A folder I choose".into(),
        });
        out.send(
            topics::CONFIG_OPTIONS,
            json!({ "key": INSTALL, "options": options }),
        );
    }

    fn launch_for(&self, start: &Value) -> Result<Launch, String> {
        let mut settings = self.settings.clone();
        if let Some(Value::Object(given)) = start.get("settings") {
            settings.extend(given.clone());
        }
        let text = |k: &str| settings.get(k).and_then(Value::as_str).unwrap_or("").trim();
        let vanilla = match text(MACHINE) {
            "" => !manifest::has_extensions(),
            BANISHED => true,
            manifest::UNBANISHED if manifest::has_extensions() => false,
            other => return Err(format!("{MACHINE}: this program cannot run {other:?}")),
        };
        let data = match text(INSTALL) {
            FOLDER => match text(FOLDER_PATH) {
                "" => return Err("choose a folder in Settings, under Game files".into()),
                dir => steam::resolve_folder(dir.as_ref())
                    .ok_or(format!("no data0.pkg in {dir} or in its WinData folder"))?,
            },
            FIRST_FOUND | "" => match self.installs.first() {
                Some(i) => i.win_data.clone(),
                None => return Err("Banished was not found in Steam; choose its folder in Settings, under Game files".into()),
            },
            dir => steam::resolve_folder(dir.as_ref())
                .ok_or(format!("Banished is no longer in {dir}"))?,
        };
        let mute = settings.get(MUTE).and_then(Value::as_bool).unwrap_or(false);
        let mut args: Vec<String> = text(ARGS).split_whitespace().map(String::from).collect();
        if let Some(Value::Array(more)) = start.get("args") {
            args.extend(more.iter().filter_map(Value::as_str).map(String::from));
        }
        Ok(Launch {
            data,
            vanilla,
            mute,
            args,
        })
    }
}

impl Game for Kernel {
    fn manifest(&self) -> Manifest {
        manifest::manifest()
    }

    fn on_event(&mut self, e: &Event, out: &mut Outbox) {
        match e.topic.as_str() {
            topics::FRAME_READY => {
                if let Some(Value::Object(s)) = e.data.get("settings") {
                    self.settings = s.clone();
                }
                self.offer_installs(out);
            }
            topics::CONFIG_SET => {
                if let Some(key) = e.data.get("key").and_then(Value::as_str) {
                    self.settings.insert(key.into(), e.data["value"].clone());
                }
            }
            topics::GAME_START => match self.launch_for(&e.data) {
                Ok(l) => {
                    let machine = if l.vanilla { "Banished" } else { "Un-Banished" };
                    out.status("loading", &format!("{machine} from {}", l.data.display()));
                    self.launch = Some(l);
                }
                Err(err) => out.status("failed", &err),
            },
            topics::GAME_STOP if self.launch.take().is_some() => {
                out.status("stopped", "stopped before it started")
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "kernel/tests.rs"]
mod tests;

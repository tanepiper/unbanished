//! What the game tells the frame about itself (GameFrame's manifest, contract 1.1): one
//! renderer, and the settings that make up a launch. The frame draws only these.

use gameframe_frame::{
    capabilities, Choice, Grade, Kind, Manifest, OptionsFrom, Renderer, Setting, CONTRACT,
};
use serde_json::json;

/// Which game runs: [`BANISHED`] or [`UNBANISHED`].
pub(crate) const MACHINE: &str = "machine";
/// Banished 1.0.7 exactly: the core and the original's own mods, every extension off.
pub(crate) const BANISHED: &str = "banished";
/// Un-Banished: the core with the extensions built into this program.
pub(crate) const UNBANISHED: &str = "unbanished";
/// Which copy of Banished: a `WinData` folder the kernel found, or [`FOLDER`].
pub(crate) const INSTALL: &str = "data.install";
/// The value of [`INSTALL`] for the first copy found, wherever it is now.
pub(crate) const FIRST_FOUND: &str = "found";
/// The value of [`INSTALL`] that reads [`FOLDER_PATH`].
pub(crate) const FOLDER: &str = "folder";
/// The folder the player chose.
pub(crate) const FOLDER_PATH: &str = "data.folder";
pub(crate) const MUTE: &str = "sound.mute";
/// More of `unbanished`'s own options, split at spaces.
pub(crate) const ARGS: &str = "game.args";

/// The renderer's id: the port draws with wgpu on every system.
pub(crate) const WGPU: &str = "wgpu";

/// Whether this program has extensions to turn on: a build without them offers Banished only.
pub(crate) fn has_extensions() -> bool {
    cfg!(feature = "more")
}

pub(crate) fn manifest() -> Manifest {
    Manifest {
        contract: CONTRACT.into(),
        name: if has_extensions() { "Un-Banished More" } else { "Un-Banished" }.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        capabilities: [
            capabilities::START,
            capabilities::RENDERERS,
            capabilities::SETTINGS,
        ]
        .map(String::from)
        .into(),
        renderers: vec![Renderer {
            id: WGPU.into(),
            label: "wgpu".into(),
            detail: Some("Metal on macOS, Vulkan or Direct3D 12 elsewhere".into()),
            grade: Grade::Good,
            notes: vec![
                "Draws the original's materials and shaders, ported to WGSL. The open differences are in DEVIATIONS.md.".into(),
            ],
            default: true,
            ..Renderer::default()
        }],
        devices: Vec::new(),
        settings: settings(),
        controls: Vec::new(),
    }
}

fn choice(value: &str, label: &str) -> Choice {
    Choice {
        value: value.into(),
        label: label.into(),
    }
}

fn settings() -> Vec<Setting> {
    let mut machines = vec![choice(BANISHED, "Banished 1.0.7, exactly the original")];
    if has_extensions() {
        machines.insert(0, choice(UNBANISHED, "Un-Banished, with its extensions"));
    }
    let default_machine = machines[0].value.clone();
    vec![
        Setting {
            key: MACHINE.into(),
            label: "Game".into(),
            help: Some(
                "Banished plays the original game and its own mods only. Un-Banished adds multiplayer, the mod sites, recordings and rule hooks.".into(),
            ),
            group: Some("Game".into()),
            kind: Kind::Choice {
                options: machines,
                from: None,
            },
            default: json!(default_machine),
            restart: false,
        },
        Setting {
            key: INSTALL.into(),
            label: "Banished's files".into(),
            help: Some("Your own copy of Banished, found in Steam, or a folder you choose.".into()),
            group: Some("Game files".into()),
            kind: Kind::Choice {
                options: vec![
                    choice(FIRST_FOUND, "The first copy found in Steam"),
                    choice(FOLDER, "A folder I choose"),
                ],
                from: Some(OptionsFrom {
                    files: None,
                    game: true,
                }),
            },
            default: json!(FIRST_FOUND),
            restart: false,
        },
        Setting {
            key: FOLDER_PATH.into(),
            label: "Folder".into(),
            help: Some(
                "For \"A folder I choose\": the Banished folder, its WinData folder, or any folder with data0.pkg.".into(),
            ),
            group: Some("Game files".into()),
            kind: Kind::Text,
            default: json!(""),
            restart: false,
        },
        Setting {
            key: MUTE.into(),
            label: "Start with the sound off".into(),
            help: None,
            group: Some("Sound".into()),
            kind: Kind::Toggle,
            default: json!(false),
            restart: false,
        },
        Setting {
            key: ARGS.into(),
            label: "More options".into(),
            help: Some("Options for the unbanished program, such as --ui-scale 2 or --seed 1234.".into()),
            group: Some("Advanced".into()),
            kind: Kind::Text,
            default: json!(""),
            restart: false,
        },
    ]
}

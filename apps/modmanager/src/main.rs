//! The Un-Banished mod manager (`banished-mods`): a small desktop tool to turn mods on and off
//! and set their order before the game starts, keep named mod lists, install mods from Nexus
//! Mods, and start the game.
//!
//! It is not part of the original game. It reads the `.pkm` packages in the pack directory
//! (`WinData`) and writes the same settings the game's Mods dialog writes (`ModEnabled_<file>`,
//! `ModIndex_<file>` in the settings file), through the shared `banished-mods` crate.
//!
//! ```text
//! banished-mods [path/to/WinData]
//! ```

mod app;
mod launch;
mod lists;
mod nexus_tab;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Un-Banished mods")
            .with_inner_size([980.0, 640.0])
            .with_min_inner_size([720.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Un-Banished mods",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, &args)))),
    )
}

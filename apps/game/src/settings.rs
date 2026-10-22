//! Persisted settings (`IO::Persistence`, Runtime `0x1800ba880`) and the four configurations the
//! Options dialog edits: `Video::Configuration` (`0x1800f9760`), `Audio::Configuration`
//! (`0x1800e0700`), `Input::Configuration` (`0x1800e3630`) and the game's options
//! (`DAT_1401cbde0`, loaded by `FUN_14009e030`, saved by `FUN_14009de40`).
//!
//! The original keeps the values in the Windows registry under names made with `"%s%s"`
//! (section then key: `VideoWidth`, `AudioMusicVolume`, `InputMouseLeft`). The port keeps the
//! same names in a text file (`name=value` lines) in the platform's configuration directory, or
//! in the browser's local storage.

mod display;
mod video;
pub use display::{Adapter, DisplayInfo, DisplayMode, GraphicsInterface};
pub use video::VideoConfig;

use std::collections::BTreeMap;

/// The settings store.
#[derive(Debug, Clone, Default)]
pub struct Persistence {
    values: BTreeMap<String, String>,
    /// Read from the store, so writes go back to it; a store made in memory (captures, tests)
    /// is never written.
    stored: bool,
}

/// The mod list (`unbanished_mods`) reads and writes the same values.
impl unbanished_mods::Settings for Persistence {
    fn get_bool(&self, section: &str, name: &str, value: &mut bool) {
        Persistence::get_bool(self, section, name, value);
    }
    fn get_int(&self, section: &str, name: &str, value: &mut i32) {
        Persistence::get_int(self, section, name, value);
    }
    fn set_bool(&mut self, section: &str, name: &str, value: bool) {
        Persistence::set_bool(self, section, name, value);
    }
    fn set_int(&mut self, section: &str, name: &str, value: i32) {
        Persistence::set_int(self, section, name, value);
    }
}

impl Persistence {
    /// `ReadSettings`: everything stored, or nothing on first run.
    pub fn read() -> Persistence {
        let mut p = Persistence {
            stored: true,
            ..Persistence::default()
        };
        if let Some(text) = storage::read() {
            p.parse(&text);
        }
        p
    }

    /// `WriteSettings`.
    pub fn write(&self) {
        if self.stored {
            storage::write(&self.serialize());
        }
    }

    pub fn parse(&mut self, text: &str) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                self.values
                    .insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }

    pub fn serialize(&self) -> String {
        self.values
            .iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect()
    }

    fn key(section: &str, name: &str) -> String {
        format!("{section}{name}")
    }

    /// `Persistence::Get(section, key, int&)`: unchanged when absent or unreadable.
    pub fn get_int(&self, section: &str, name: &str, value: &mut i32) {
        if let Some(v) = self.values.get(&Self::key(section, name)) {
            if let Ok(n) = v.parse() {
                *value = n;
            }
        }
    }

    /// A text value, or `None` when absent or empty.
    pub fn get_text(&self, section: &str, name: &str) -> Option<String> {
        self.values
            .get(&Self::key(section, name))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    pub fn set_text(&mut self, section: &str, name: &str, value: &str) {
        self.values
            .insert(Self::key(section, name), value.to_string());
    }

    /// `Persistence::Get(section, key, bool&)`: `"true"` is true, anything else false.
    pub fn get_bool(&self, section: &str, name: &str, value: &mut bool) {
        if let Some(v) = self.values.get(&Self::key(section, name)) {
            *value = v == "true";
        }
    }

    pub fn set_int(&mut self, section: &str, name: &str, value: i32) {
        self.values
            .insert(Self::key(section, name), value.to_string());
    }

    pub fn set_bool(&mut self, section: &str, name: &str, value: bool) {
        self.values.insert(
            Self::key(section, name),
            if value { "true" } else { "false" }.to_string(),
        );
    }
}

/// `Audio::Configuration` (`0x1800e0700`): volumes 0–100.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioConfig {
    pub ui: i32,
    pub music: i32,
    pub ambient: i32,
    pub effect: i32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        AudioConfig {
            ui: 100,
            music: 50,
            ambient: 100,
            effect: 100,
        }
    }
}

impl AudioConfig {
    /// `Audio::Configuration::Startup` (`0x1800e0730`).
    pub fn load(p: &Persistence) -> AudioConfig {
        let mut c = AudioConfig::default();
        p.get_int("Audio", "UIVolume", &mut c.ui);
        p.get_int("Audio", "MusicVolume", &mut c.music);
        p.get_int("Audio", "AmbientVolume", &mut c.ambient);
        p.get_int("Audio", "EffectVolume", &mut c.effect);
        c
    }

    pub fn save(&self, p: &mut Persistence) {
        p.set_int("Audio", "UIVolume", self.ui);
        p.set_int("Audio", "MusicVolume", self.music);
        p.set_int("Audio", "AmbientVolume", self.ambient);
        p.set_int("Audio", "EffectVolume", self.effect);
    }
}

/// `Input::Configuration` (`0x1800e3630`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InputConfig {
    /// `+8`: keep the pointer inside the window.
    pub mouse_clipping: bool,
    /// `+9`.
    pub legacy: bool,
    /// `+10`: draw the pointer as a sprite ("Use Software Mouse").
    pub mouse_display: bool,
}

impl InputConfig {
    pub fn load(p: &Persistence) -> InputConfig {
        let mut c = InputConfig::default();
        p.get_bool("Input", "MouseClipping", &mut c.mouse_clipping);
        p.get_bool("Input", "Legacy", &mut c.legacy);
        p.get_bool("Input", "MouseDisplay", &mut c.mouse_display);
        c
    }

    pub fn save(&self, p: &mut Persistence) {
        p.set_bool("Input", "MouseClipping", self.mouse_clipping);
        p.set_bool("Input", "Legacy", self.legacy);
        p.set_bool("Input", "MouseDisplay", self.mouse_display);
    }
}

/// The game's options (`DAT_1401cbde0`, 0x34 bytes; defaults from the boot code at
/// `0x140052763`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameOptions {
    /// `+0`: user interface scale, percent (75–150 on the slider).
    pub ui_scale: i32,
    /// `+4`: autosave interval index (`StringTable.rsc:autoSave`, 8 disabled).
    pub autosave_interval: i32,
    /// `+8`, `+0xc`, `+0x10`.
    pub status_icon_opacity: i32,
    pub status_icon_enable: bool,
    pub status_icon_scale: i32,
    /// `+0x14..+0x18`.
    pub edge_scroll: bool,
    pub show_celsius: bool,
    pub show_weather: bool,
    pub auto_slow: bool,
    pub auto_pause: bool,
    /// `+0x1c`: the Options dialog's last tab.
    pub last_option: i32,
    /// `+0x20..+0x30`: New Game choices.
    pub last_terrain: i32,
    pub last_map_size: i32,
    pub last_climate: i32,
    pub last_start: i32,
    pub last_disaster: i32,
}

impl Default for GameOptions {
    fn default() -> Self {
        GameOptions {
            ui_scale: 100,
            autosave_interval: 1,
            status_icon_opacity: 100,
            status_icon_enable: true,
            status_icon_scale: 100,
            edge_scroll: true,
            show_celsius: false,
            show_weather: true,
            auto_slow: true,
            auto_pause: true,
            last_option: 0,
            last_terrain: 0,
            last_map_size: 0,
            last_climate: 1,
            last_start: 1,
            last_disaster: 0,
        }
    }
}

impl GameOptions {
    /// `FUN_14009e030`.
    pub fn load(p: &Persistence) -> GameOptions {
        let mut c = GameOptions::default();
        let s = "Game";
        p.get_int(s, "UIScale", &mut c.ui_scale);
        p.get_int(s, "AutosaveInterval", &mut c.autosave_interval);
        p.get_int(s, "StatusIconOpacity", &mut c.status_icon_opacity);
        p.get_bool(s, "StatusIconEnable", &mut c.status_icon_enable);
        p.get_int(s, "StatusIconScale", &mut c.status_icon_scale);
        p.get_bool(s, "ShowCelcius", &mut c.show_celsius);
        p.get_bool(s, "EdgeScroll", &mut c.edge_scroll);
        p.get_bool(s, "ShowWeather", &mut c.show_weather);
        p.get_bool(s, "AutoSlow", &mut c.auto_slow);
        p.get_bool(s, "AutoPause", &mut c.auto_pause);
        p.get_int(s, "LastOption", &mut c.last_option);
        p.get_int(s, "LastTerrain", &mut c.last_terrain);
        p.get_int(s, "LastMapSize", &mut c.last_map_size);
        p.get_int(s, "LastClimate", &mut c.last_climate);
        p.get_int(s, "LastStart", &mut c.last_start);
        p.get_int(s, "LastDisaster", &mut c.last_disaster);
        c
    }

    /// `FUN_14009de40`.
    pub fn save(&self, p: &mut Persistence) {
        let s = "Game";
        p.set_int(s, "UIScale", self.ui_scale);
        p.set_int(s, "AutosaveInterval", self.autosave_interval);
        p.set_int(s, "StatusIconOpacity", self.status_icon_opacity);
        p.set_bool(s, "StatusIconEnable", self.status_icon_enable);
        p.set_int(s, "StatusIconScale", self.status_icon_scale);
        p.set_bool(s, "ShowCelcius", self.show_celsius);
        p.set_bool(s, "EdgeScroll", self.edge_scroll);
        p.set_bool(s, "ShowWeather", self.show_weather);
        p.set_bool(s, "AutoSlow", self.auto_slow);
        p.set_bool(s, "AutoPause", self.auto_pause);
        p.set_int(s, "LastOption", self.last_option);
        p.set_int(s, "LastTerrain", self.last_terrain);
        p.set_int(s, "LastMapSize", self.last_map_size);
        p.set_int(s, "LastClimate", self.last_climate);
        p.set_int(s, "LastStart", self.last_start);
        p.set_int(s, "LastDisaster", self.last_disaster);
    }
}

/// Everything the Options dialog edits, loaded together at startup.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub persistence: Persistence,
    pub video: VideoConfig,
    pub audio: AudioConfig,
    pub input: InputConfig,
    pub game: GameOptions,
    /// `Input::KeyMap`, stored with the input configuration.
    pub keymap: crate::keymap::KeyMap,
    /// What the video device offers (`Video::Device` interfaces, adapters and modes), filled
    /// by the platform layer; not stored.
    pub display: DisplayInfo,
    /// Bumped whenever the Options dialog applies or reverts settings, so the platform layer
    /// re-applies the video and input configurations.
    pub revision: u32,
}

impl Settings {
    pub fn load() -> Settings {
        let persistence = Persistence::read();
        Settings {
            video: VideoConfig::load(&persistence),
            audio: AudioConfig::load(&persistence),
            input: InputConfig::load(&persistence),
            game: GameOptions::load(&persistence),
            keymap: crate::keymap::KeyMap::load(&persistence),
            persistence,
            display: DisplayInfo::default(),
            revision: 0,
        }
    }

    /// Store every configuration and write the settings out.
    pub fn save(&mut self) {
        self.video.save(&mut self.persistence);
        self.audio.save(&mut self.persistence);
        self.input.save(&mut self.persistence);
        self.keymap.save(&mut self.persistence);
        self.game.save(&mut self.persistence);
        self.persistence.write();
    }
}

// Deviation: the original keeps these values in the registry; the port writes the same names to
// a settings file natively and to localStorage in the browser.
#[cfg(not(target_arch = "wasm32"))]
mod storage {
    use std::path::PathBuf;

    /// The platform configuration directory: `%APPDATA%`, `~/Library/Application Support` or
    /// `$XDG_CONFIG_HOME` (`~/.config`). `UNBANISHED_SETTINGS` names the file directly (tests,
    /// captures).
    fn path() -> Option<PathBuf> {
        if let Ok(p) = unbanished_formats::env::var("SETTINGS") {
            return (!p.is_empty()).then(|| PathBuf::from(p));
        }
        let base = if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        }?;
        Some(base.join("banished-wasm").join("settings.txt"))
    }

    pub fn read() -> Option<String> {
        std::fs::read_to_string(path()?).ok()
    }

    pub fn write(text: &str) {
        let Some(path) = path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&path, text) {
            log::warn!("{}: {e}", path.display());
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod storage {
    const KEY: &str = "banished-wasm.settings";

    fn local() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn read() -> Option<String> {
        local()?.get_item(KEY).ok()?
    }

    pub fn write(text: &str) {
        if let Some(s) = local() {
            let _ = s.set_item(KEY, text);
        }
    }
}

#[cfg(test)]
mod tests;

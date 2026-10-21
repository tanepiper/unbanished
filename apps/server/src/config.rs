//! The server's settings: the command line, and a settings file of `key = value` lines.
//!
//! ```text
//! # unbanished-server.conf
//! address = 0.0.0.0:7780     # where to listen
//! max_rooms = 100            # the most rooms open at once
//! rooms = /var/lib/unbanished  # keep each room's town here (none: rooms are not kept)
//! simulate = /srv/banished/WinData  # run each room's game with this game data (none: don't)
//! ```

/// The server's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Where to listen.
    pub address: String,
    /// The most rooms open at once.
    pub max_rooms: usize,
    /// The folder that keeps each room's town between games; none keeps nothing.
    pub rooms: Option<std::path::PathBuf>,
    /// The game data (`WinData`) to run each room's game with; none runs no game.
    pub simulate: Option<std::path::PathBuf>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            address: "0.0.0.0:7780".into(),
            max_rooms: 100,
            rooms: None,
            simulate: None,
        }
    }
}

impl Config {
    /// The settings from the command line: `[ADDRESS] [--config FILE] [--simulate DIR]`. The file's values come
    /// first; an address on the command line replaces its address.
    pub fn from_args(args: &[String]) -> Result<Config, String> {
        let mut config = Config::default();
        let mut address = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--config" => {
                    let path = args.get(i + 1).ok_or("--config needs a file")?;
                    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
                    config.read(&text).map_err(|e| format!("{path}: {e}"))?;
                    i += 1;
                }
                "--simulate" => {
                    let dir = args
                        .get(i + 1)
                        .ok_or("--simulate needs the game data folder")?;
                    config.simulate = Some(dir.into());
                    i += 1;
                }
                a if a.starts_with("--") => return Err(format!("unknown option {a}")),
                a => address = Some(a.to_string()),
            }
            i += 1;
        }
        if let Some(a) = address {
            config.address = a;
        }
        Ok(config)
    }

    /// Reads a settings file's lines into these settings.
    pub fn read(&mut self, text: &str) -> Result<(), String> {
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("line {}: not `key = value`", n + 1))?;
            let value = value.trim();
            match key.trim() {
                "address" => self.address = value.to_string(),
                "max_rooms" => {
                    self.max_rooms = value
                        .parse()
                        .map_err(|_| format!("line {}: max_rooms is a number", n + 1))?
                }
                "rooms" => self.rooms = Some(value.into()),
                "simulate" => self.simulate = Some(value.into()),
                other => return Err(format!("line {}: unknown setting {other}", n + 1)),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_file_and_the_command_line() {
        let mut c = Config::default();
        c.read("# comment\naddress = 127.0.0.1:9000  # here\n\nmax_rooms = 3\nrooms = kept\n")
            .unwrap();
        assert_eq!(c.rooms.as_deref(), Some(std::path::Path::new("kept")));
        assert_eq!(c.address, "127.0.0.1:9000");
        assert_eq!(c.max_rooms, 3);
        assert!(c.read("colour = blue").is_err());
        assert!(c.read("max_rooms = many").is_err());
        let args = ["0.0.0.0:1".to_string()];
        assert_eq!(Config::from_args(&args).unwrap().address, "0.0.0.0:1");
        assert!(Config::from_args(&["--nope".to_string()]).is_err());
    }
}

//! `nxm://` links. The website's "Mod Manager Download" button gives one for a file:
//! `nxm://banished/mods/19/files/49?key=…&expires=…&user_id=…`. Its `key` and `expires` let
//! an account without Premium get that file's download link for a short time.

use crate::{Error, GAME_DOMAIN};

/// A parsed `nxm://` link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NxmLink {
    /// The game's domain (`banished`).
    pub game: String,
    pub mod_id: u32,
    pub file_id: u32,
    /// The one-time key and its expiry time (Unix seconds), passed to the download link.
    pub key: String,
    pub expires: String,
    pub user_id: Option<String>,
}

impl NxmLink {
    /// Read a link. It must be for Banished and carry a key and an expiry time.
    pub fn parse(link: &str) -> Result<NxmLink, Error> {
        let bad = |why: &str| Error::Answer(format!("not a Banished nxm:// link: {why}"));
        let rest = link
            .trim()
            .strip_prefix("nxm://")
            .ok_or_else(|| bad("it does not start with nxm://"))?;
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        let [game, "mods", mod_id, "files", file_id] = parts[..] else {
            return Err(bad("expected nxm://banished/mods/ID/files/ID"));
        };
        if !game.eq_ignore_ascii_case(GAME_DOMAIN) {
            return Err(bad(&format!("it is for {game}")));
        }
        let param = |name: &str| {
            query
                .split('&')
                .filter_map(|p| p.split_once('='))
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        };
        Ok(NxmLink {
            game: game.to_ascii_lowercase(),
            mod_id: mod_id
                .parse()
                .map_err(|_| bad("the mod id is not a number"))?,
            file_id: file_id
                .parse()
                .map_err(|_| bad("the file id is not a number"))?,
            key: param("key").ok_or_else(|| bad("it has no key"))?,
            expires: param("expires").ok_or_else(|| bad("it has no expiry time"))?,
            user_id: param("user_id"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_link() {
        let l =
            NxmLink::parse("nxm://Banished/mods/19/files/49?key=k1&expires=1700000000&user_id=5")
                .unwrap();
        assert_eq!((l.mod_id, l.file_id), (19, 49));
        assert_eq!(l.key, "k1");
        assert_eq!(l.expires, "1700000000");
        assert_eq!(l.user_id.as_deref(), Some("5"));
        assert!(NxmLink::parse("nxm://skyrim/mods/1/files/2?key=a&expires=1").is_err());
        assert!(NxmLink::parse("nxm://banished/mods/1/files/2").is_err());
        assert!(NxmLink::parse("https://www.nexusmods.com/banished/mods/1").is_err());
    }
}

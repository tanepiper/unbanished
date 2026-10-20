//! Optional [banishedinfo.com](https://banishedinfo.com/mods/browse) support for Un-Banished:
//! list and search the site's Banished mods, read a mod's versions, and install the `.pkm`
//! packages of a version. It is not part of the original game.
//!
//! The site has no public API. Its mod list loads from a JSON address the browse page itself
//! uses (`/mods/ajax?action=mods&…`), its search page carries its results as JSON in the page
//! (`var mods = […]`), and a mod's page links each version (`/mods/download/<mod>/<n>`), whose
//! file (`/mods/file/<mod>/<n>`) redirects to the package or archive. No account is needed.
//!
//! Requests are made only when the player lists, searches or installs, one at a time, with a
//! user agent that names Un-Banished ([`user_agent`]). The site sends no CORS headers, so a
//! browser page cannot read it: the support is for native builds (`native`).
//!
//! This crate builds the addresses and reads the answers; [`native`] sends the requests.

use serde_json::Value;

#[cfg(feature = "native")]
pub mod native;

/// The site.
pub const SITE: &str = "https://banishedinfo.com";

/// The user agent sent with every request: the application, its version and where it lives.
pub fn user_agent(version: &str) -> String {
    format!("Un-Banished/{version} (+https://github.com/tanepiper/unbanished)")
}

/// The order of the site's list (its `sort` values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// The site's own overview order (`overview`).
    Featured,
    /// Most downloaded (`downloads`).
    Downloads,
    /// Most downloaded in the last day (`downloads24`).
    Trending,
    /// Newest first (`recent`).
    Recent,
}

impl Sort {
    fn value(self) -> &'static str {
        match self {
            Sort::Featured => "overview",
            Sort::Downloads => "downloads",
            Sort::Trending => "downloads24",
            Sort::Recent => "recent",
        }
    }
}

/// The site's mod sections (`section` values), or all of them.
pub const SECTIONS: [(&str, &str); 8] = [
    ("", "All"),
    ("audio", "Audio"),
    ("buildings", "Buildings"),
    ("gameplay", "Gameplay"),
    ("maps", "Maps"),
    ("textures", "Textures"),
    ("translations", "Translations"),
    ("translated-mods", "Translated mods"),
];

/// A mod as the site lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Mod {
    /// The mod's id on the site.
    pub id: u32,
    /// The mod's part of its address: `679-CC-New-Frontier-BlackLiquid`.
    pub url: String,
    /// The mod's title.
    pub title: String,
    /// Its author.
    pub author: String,
    /// The start of its description.
    pub info: String,
    /// The version the author gave the newest upload (`1.55`).
    pub version: String,
    /// How many times it was downloaded.
    pub downloads: u64,
    /// The share of players who liked it, 0 to 100.
    pub rating: u8,
    /// Its thumbnail image (a PNG), as a full address.
    pub thumbnail: Option<String>,
    /// When it was created (`2014-10-26 05:07:55`).
    pub created: String,
}

impl Mod {
    /// A mod known only by its id (its page answers without the name).
    pub fn with_id(id: u32) -> Mod {
        Mod {
            id,
            url: id.to_string(),
            title: format!("mod {id}"),
            author: String::new(),
            info: String::new(),
            version: String::new(),
            downloads: 0,
            rating: 0,
            thumbnail: None,
            created: String::new(),
        }
    }

    /// The mod's page.
    pub fn page(&self) -> String {
        format!("{SITE}/mods/view/{}", self.url)
    }
}

/// One version of a mod, newest first in [`parse_versions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// The version as the page names it (`1.1`).
    pub label: String,
    /// The address of its file, which redirects to the download.
    pub file: String,
}

/// What can go wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The request failed or the answer could not be read (the message says why).
    Network(String),
    /// The site answered with this HTTP status.
    Status(u16),
    /// The page or data was not what was expected (the site may have changed).
    Answer(String),
    /// The download could not be installed (no `.pkm`, or a `.7z` or `.rar` archive).
    Install(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Network(m) => write!(f, "could not reach banishedinfo.com: {m}"),
            Error::Status(s) => write!(f, "banishedinfo.com answered {s}"),
            Error::Answer(m) => write!(f, "unexpected answer from banishedinfo.com: {m}"),
            Error::Install(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

/// Percent-encode a value for an address.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The address of `count` mods from `start` of a section (`""` for all), in an order.
pub fn list_url(sort: Sort, section: &str, start: u32, count: u32) -> String {
    format!(
        "{SITE}/mods/ajax?action=mods&start={start}&count={count}&section={}&sort={}",
        encode(section),
        sort.value()
    )
}

/// The address of the site's search for `text`.
pub fn search_url(text: &str) -> String {
    format!("{SITE}/search?query={}", encode(text.trim()))
}

/// A number in the site's text form (`"291,278 downloads"`, `1.55`, `98`).
fn count(v: &Value) -> u64 {
    if let Some(n) = v.as_u64() {
        return n;
    }
    v.as_str()
        .map(|s| s.chars().take_while(|c| c.is_ascii_digit() || *c == ','))
        .map(|c| c.filter(char::is_ascii_digit).collect::<String>())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn mod_of(v: &Value) -> Option<Mod> {
    let url = text(&v["url"]);
    let id = count(&v["mod_id"]) as u32;
    if url.is_empty() || id == 0 {
        return None;
    }
    let thumb = text(&v["thumb"]);
    Some(Mod {
        id,
        url,
        title: unescape(&text(&v["title"])),
        author: unescape(&text(&v["author"])),
        info: unescape(&text(&v["info"])),
        version: text(&v["version"]),
        downloads: count(&v["downloads"]),
        rating: count(&v["rating"]).min(100) as u8,
        thumbnail: (!thumb.is_empty()).then(|| {
            if thumb.starts_with('/') {
                format!("{SITE}{thumb}")
            } else {
                thumb
            }
        }),
        created: text(&v["created"]),
    })
}

/// Read the answer of [`list_url`]: a JSON list of mods (empty past the end).
pub fn parse_list(body: &[u8]) -> Result<Vec<Mod>, Error> {
    let v: Value = serde_json::from_slice(body).map_err(|e| Error::Answer(e.to_string()))?;
    let list = v
        .as_array()
        .ok_or_else(|| Error::Answer("the mod list is not a list".into()))?;
    Ok(list.iter().filter_map(mod_of).collect())
}

/// Read the search page of [`search_url`]: the results it carries as `var mods = […];`.
pub fn parse_search(page: &str) -> Result<Vec<Mod>, Error> {
    let Some(start) = page.find("var mods = ") else {
        // A search without results has no list.
        return Ok(Vec::new());
    };
    let rest = &page[start + "var mods = ".len()..];
    let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
    let v = stream
        .next()
        .ok_or_else(|| Error::Answer("no results list".into()))?
        .map_err(|e| Error::Answer(e.to_string()))?;
    Ok(v.as_array()
        .map(|a| a.iter().filter_map(mod_of).collect())
        .unwrap_or_default())
}

/// The few HTML entities mod titles use.
fn unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&ntilde;", "ñ")
        .replace("&nbsp;", " ")
}

/// Read a mod's page ([`Mod::page`]): its versions, newest first, from the links
/// `/mods/download/<id>-<name>/<n>">Download version <label>` (the page of mod `id`).
pub fn parse_versions(page: &str, id: u32) -> Vec<Version> {
    let prefix = format!("/mods/download/{id}-");
    let mut out: Vec<Version> = Vec::new();
    let mut rest = page;
    while let Some(i) = rest.find(&prefix) {
        rest = &rest[i + prefix.len()..];
        let Some((slug, after)) = rest.split_once('/') else {
            break;
        };
        let n: String = after.chars().take_while(char::is_ascii_digit).collect();
        let label = after[n.len()..]
            .strip_prefix("\">Download version ")
            .and_then(|t| t.split('<').next())
            .map(|t| unescape(t.trim()));
        if let (false, false, Some(label)) = (n.is_empty(), slug.contains(['"', '<', ' ']), label) {
            let file = format!("{SITE}/mods/file/{id}-{slug}/{n}");
            if !out.iter().any(|v| v.file == file) {
                out.push(Version { label, file });
            }
        }
    }
    out
}

/// The file name at the end of a download's address (`/mods/file-download/264/XP-Schools.pkm`).
pub fn file_name(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .and_then(|u| u.rsplit('/').next())
        .map(|n| n.replace("%20", " "))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        assert_eq!(
            list_url(Sort::Downloads, "", 20, 20),
            "https://banishedinfo.com/mods/ajax?action=mods&start=20&count=20&section=&sort=downloads"
        );
        assert_eq!(
            search_url(" more terrain&x "),
            "https://banishedinfo.com/search?query=more+terrain%26x"
        );
        assert!(user_agent("0.2.0").starts_with("Un-Banished/0.2.0 (+https://"));
    }

    const ITEM: &str = r#"{"mod_id":679,"title":"CC New Frontier &amp; More","info":"A complete expansion.",
        "author":"BlackLiquid","downloads":"291,278 downloads","rating":98,"version":1.55,
        "url":"679-CC-New-Frontier-BlackLiquid","created":"2014-10-26 05:07:55","thumb":"/images/mod-thumbs/213.png"}"#;

    #[test]
    fn reads_the_list() {
        let body = format!("[{ITEM},{{\"mod_id\":0}}]");
        let mods = parse_list(body.as_bytes()).unwrap();
        assert_eq!(mods.len(), 1);
        let m = &mods[0];
        assert_eq!((m.id, m.downloads, m.rating), (679, 291_278, 98));
        assert_eq!(m.title, "CC New Frontier & More");
        assert_eq!(m.version, "1.55");
        assert_eq!(
            m.thumbnail.as_deref(),
            Some("https://banishedinfo.com/images/mod-thumbs/213.png")
        );
        assert_eq!(
            m.page(),
            "https://banishedinfo.com/mods/view/679-CC-New-Frontier-BlackLiquid"
        );
        assert_eq!(parse_list(b"[]").unwrap(), Vec::new());
    }

    #[test]
    fn reads_search_results_from_the_page() {
        let page = format!("<script>var mods = [{ITEM}];\nfunction f() {{}}</script>");
        assert_eq!(parse_search(&page).unwrap()[0].id, 679);
        assert!(parse_search("<html>no results</html>").unwrap().is_empty());
    }

    #[test]
    fn reads_versions_newest_first() {
        let page = r#"<a href="/mods/download/34-Better-Schools/828" class="download">Download mod</a>
            <a href="/mods/download/34-Better-Schools/828">Download version 1.1</a>
            <a href="/mods/download/34-Better-Schools/99">Download version 1.0</a>"#;
        let v = parse_versions(page, 34);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].label, "1.1");
        assert_eq!(
            v[0].file,
            "https://banishedinfo.com/mods/file/34-Better-Schools/828"
        );
        assert_eq!(v[1].label, "1.0");
        assert_eq!(
            file_name("https://banishedinfo.com/mods/file-download/264/XP-Schools.pkm"),
            "XP-Schools.pkm"
        );
    }
}

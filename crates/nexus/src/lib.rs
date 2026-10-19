#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

pub mod api;
pub mod install;
pub mod nxm;

#[cfg(feature = "native")]
pub mod native;

/// The game's domain name on Nexus Mods, used in its addresses.
pub const GAME_DOMAIN: &str = "banished";

/// The game's id on Nexus Mods, used by the GraphQL API.
pub const GAME_ID: u32 = 617;

/// The REST API (version 1), which needs an API key.
pub const API_V1: &str = "https://api.nexusmods.com/v1";

/// The GraphQL API (version 2), which answers searches without a key.
pub const API_V2: &str = "https://api.nexusmods.com/v2/graphql";

/// The application that sends the requests, as Nexus Mods asks every application to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// The application's name, the same in every version: `Un-Banished`.
    pub name: String,
    /// The application's version, for example `0.2.0`.
    pub version: String,
}

impl App {
    /// Un-Banished at the given engine version.
    pub fn un_banished(version: &str) -> App {
        App {
            name: "Un-Banished".to_string(),
            version: version.to_string(),
        }
    }
}

/// An HTTP request to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// `GET` or `POST`.
    pub method: &'static str,
    /// The full address.
    pub url: String,
    /// The headers, as name and value.
    pub headers: Vec<(String, String)>,
    /// The body (JSON for GraphQL), if any.
    pub body: Option<String>,
}

/// Something that went wrong with Nexus Mods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The request needs an API key and none is set.
    NoKey,
    /// The request could not be sent or the answer could not be read (the message says why).
    Network(String),
    /// Nexus Mods answered with an error: the HTTP status and its message.
    Status(u16, String),
    /// The answer was not what was expected.
    Answer(String),
    /// Downloads of this file need Premium, or a link from the website's "Mod Manager
    /// Download" button.
    NeedsPremium,
    /// The archive is in a format that cannot be opened here (its extension).
    Unsupported(String),
    /// The download holds no `.pkm` package.
    NoPackage,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoKey => write!(f, "no Nexus Mods API key is set"),
            Error::Network(m) => write!(f, "could not reach Nexus Mods: {m}"),
            Error::Status(s, m) => write!(f, "Nexus Mods answered {s}: {m}"),
            Error::Answer(m) => write!(f, "unexpected answer from Nexus Mods: {m}"),
            Error::NeedsPremium => write!(
                f,
                "this download needs Nexus Mods Premium, or a link from the mod page's \
                 \"Mod Manager Download\" button"
            ),
            Error::Unsupported(e) => write!(
                f,
                "a .{e} archive cannot be opened here; download it from Nexus Mods and put its \
                 .pkm file in the WinData folder"
            ),
            Error::NoPackage => write!(f, "the download holds no .pkm mod package"),
        }
    }
}

impl std::error::Error for Error {}

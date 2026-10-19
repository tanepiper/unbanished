//! The Nexus Mods requests Un-Banished sends, and how their answers are read.
//!
//! Searches and file lists use the public GraphQL API ([`API_V2`]), which needs no key. The
//! player's account and download links use the REST API ([`API_V1`]) with the `apikey` header.

use serde_json::{json, Value};

use crate::{App, Error, Request, API_V1, API_V2, GAME_DOMAIN, GAME_ID};

/// The order of search results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// Most downloaded first.
    Downloads,
    /// Most endorsed first.
    Endorsements,
    /// Most recently updated first.
    Updated,
    /// By name, A to Z.
    Name,
}

/// One mod in a search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModSummary {
    /// The mod's id on Nexus Mods (in its address, `/banished/mods/ID`).
    pub mod_id: u32,
    pub name: String,
    pub author: String,
    /// The short description.
    pub summary: String,
    pub version: String,
    pub downloads: u64,
    pub endorsements: u64,
    /// When it was last updated, as ISO 8601 (`2015-11-21T11:05:01Z`).
    pub updated: String,
    /// The address of its small preview image, if it has one.
    pub thumbnail: Option<String>,
}

impl ModSummary {
    /// The mod's page on the website.
    pub fn page(&self) -> String {
        mod_page(self.mod_id)
    }
}

/// The address of a mod's page on the website.
pub fn mod_page(mod_id: u32) -> String {
    format!("https://www.nexusmods.com/{GAME_DOMAIN}/mods/{mod_id}")
}

/// The address of a mod's files tab, where "Mod Manager Download" gives an `nxm://` link.
pub fn files_page(mod_id: u32) -> String {
    format!("{}?tab=files", mod_page(mod_id))
}

/// A page of search results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    pub mods: Vec<ModSummary>,
    /// The number of mods that match, over all pages.
    pub total: u32,
}

/// One downloadable file of a mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModFile {
    /// The file's id, used for its download link.
    pub file_id: u32,
    /// The name the author gave the file.
    pub name: String,
    /// The file's name on disk, with its extension (`More Terrain v6-19-6.zip`).
    pub file_name: String,
    /// `MAIN`, `UPDATE`, `OPTIONAL`, `MISCELLANEOUS`, `OLD_VERSION`, …
    pub category: String,
    pub size: u64,
    pub version: String,
}

impl ModFile {
    /// The archive's extension in lower case (`zip`, `7z`, `rar`, `pkm`).
    pub fn extension(&self) -> String {
        extension(&self.file_name)
    }
}

/// A file name's extension in lower case.
pub fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// The player's account, as `users/validate` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub name: String,
    /// Premium members get download links without visiting the website.
    pub premium: bool,
}

/// The headers every request carries: the application's name and version, and the key if
/// there is one.
pub fn headers(app: &App, key: Option<&str>) -> Vec<(String, String)> {
    let mut h = vec![
        ("Application-Name".to_string(), app.name.clone()),
        ("Application-Version".to_string(), app.version.clone()),
        ("Accept".to_string(), "application/json".to_string()),
    ];
    if let Some(key) = key {
        h.push(("apikey".to_string(), key.to_string()));
    }
    h
}

fn graphql(app: &App, query: &str, variables: Value) -> Request {
    let mut headers = headers(app, None);
    headers.push(("Content-Type".to_string(), "application/json".to_string()));
    Request {
        method: "POST",
        url: API_V2.to_string(),
        headers,
        body: Some(json!({ "query": query, "variables": variables }).to_string()),
    }
}

const SEARCH: &str = "query Search($filter: ModsFilter, $sort: [ModsSort!], $offset: Int, \
    $count: Int) { mods(filter: $filter, sort: $sort, offset: $offset, count: $count) { \
    totalCount nodes { modId name summary version downloads endorsements updatedAt author \
    thumbnailUrl } } }";

/// Search the Banished mods whose name contains `text` (all of them when it is empty), in the
/// given order, `count` results from `offset`.
pub fn search(app: &App, text: &str, sort: Sort, offset: u32, count: u32) -> Request {
    let mut filter = json!({ "gameDomainName": [{ "value": GAME_DOMAIN, "op": "EQUALS" }] });
    let text = text.trim();
    if !text.is_empty() {
        filter["name"] = json!([{ "value": text, "op": "WILDCARD" }]);
    }
    let sort = match sort {
        Sort::Downloads => json!([{ "downloads": { "direction": "DESC" } }]),
        Sort::Endorsements => json!([{ "endorsements": { "direction": "DESC" } }]),
        Sort::Updated => json!([{ "updatedAt": { "direction": "DESC" } }]),
        Sort::Name => json!([{ "name": { "direction": "ASC" } }]),
    };
    graphql(
        app,
        SEARCH,
        json!({ "filter": filter, "sort": sort, "offset": offset, "count": count }),
    )
}

/// The data of a GraphQL answer, or its first error.
fn graphql_data(body: &[u8]) -> Result<Value, Error> {
    let v: Value = serde_json::from_slice(body).map_err(|e| Error::Answer(e.to_string()))?;
    if let Some(e) = v["errors"].as_array().and_then(|e| e.first()) {
        return Err(Error::Answer(
            e["message"].as_str().unwrap_or("GraphQL error").to_string(),
        ));
    }
    Ok(v["data"].clone())
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// Numbers come as JSON numbers or, for sizes, as strings.
fn number(v: &Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

/// Read the answer to [`search`].
pub fn parse_search(body: &[u8]) -> Result<SearchPage, Error> {
    let data = graphql_data(body)?;
    let mods = &data["mods"];
    let nodes = mods["nodes"]
        .as_array()
        .ok_or_else(|| Error::Answer("no mods in the search answer".into()))?;
    Ok(SearchPage {
        total: number(&mods["totalCount"]) as u32,
        mods: nodes.iter().map(summary).collect(),
    })
}

fn summary(m: &Value) -> ModSummary {
    ModSummary {
        mod_id: number(&m["modId"]) as u32,
        name: text(&m["name"]),
        author: text(&m["author"]),
        summary: text(&m["summary"]),
        version: text(&m["version"]),
        downloads: number(&m["downloads"]),
        endorsements: number(&m["endorsements"]),
        updated: text(&m["updatedAt"]),
        thumbnail: m["thumbnailUrl"].as_str().map(str::to_string),
    }
}

const MOD_FIELDS: &str =
    "modId name summary version downloads endorsements updatedAt author thumbnailUrl";

/// The mods with these ids (the ones installed from Nexus Mods), one lookup each in a single
/// request: the API joins repeated filters with AND, so a list of ids cannot be one filter.
pub fn by_ids(app: &App, ids: &[u32]) -> Request {
    let lookups: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                "m{id}: mods(filter: {{ gameId: [{{ value: \"{GAME_ID}\", op: EQUALS }}], \
                 modId: [{{ value: \"{id}\", op: EQUALS }}] }}) {{ nodes {{ {MOD_FIELDS} }} }}"
            )
        })
        .collect();
    graphql(
        app,
        &format!("query {{ {} }}", lookups.join(" ")),
        json!({}),
    )
}

/// Read the answer to [`by_ids`], in the order of `ids`; ids Nexus Mods no longer lists are
/// left out.
pub fn parse_by_ids(body: &[u8], ids: &[u32]) -> Result<Vec<ModSummary>, Error> {
    let data = graphql_data(body)?;
    Ok(ids
        .iter()
        .filter_map(|id| {
            data[format!("m{id}")]["nodes"]
                .as_array()?
                .first()
                .map(summary)
        })
        .collect())
}

const FILES: &str = "query Files($modId: ID!, $gameId: ID!) { modFiles(modId: $modId, gameId: \
    $gameId) { fileId name uri category sizeInBytes version } }";

/// The files of a mod.
pub fn files(app: &App, mod_id: u32) -> Request {
    graphql(
        app,
        FILES,
        json!({ "modId": mod_id.to_string(), "gameId": GAME_ID.to_string() }),
    )
}

/// Read the answer to [`files`]: the files that can still be downloaded, in the order Nexus
/// Mods gives them.
pub fn parse_files(body: &[u8]) -> Result<Vec<ModFile>, Error> {
    let data = graphql_data(body)?;
    let list = data["modFiles"]
        .as_array()
        .ok_or_else(|| Error::Answer("no files in the answer".into()))?;
    Ok(list
        .iter()
        .map(|f| ModFile {
            file_id: number(&f["fileId"]) as u32,
            name: text(&f["name"]),
            file_name: text(&f["uri"]),
            category: text(&f["category"]),
            size: number(&f["sizeInBytes"]),
            version: text(&f["version"]),
        })
        .filter(|f| !matches!(f.category.as_str(), "ARCHIVED" | "REMOVED" | "DELETED"))
        .collect())
}

/// The file to install when the player asks for a mod: the newest main file, else the newest
/// update, else the newest file that is not an old version.
pub fn main_file(files: &[ModFile]) -> Option<&ModFile> {
    let newest = |category: &str| {
        files
            .iter()
            .filter(|f| f.category == category)
            .max_by_key(|f| f.file_id)
    };
    newest("MAIN").or_else(|| newest("UPDATE")).or_else(|| {
        files
            .iter()
            .filter(|f| f.category != "OLD_VERSION")
            .max_by_key(|f| f.file_id)
    })
}

/// The account the key belongs to (`users/validate.json`).
pub fn validate(app: &App, key: &str) -> Request {
    Request {
        method: "GET",
        url: format!("{API_V1}/users/validate.json"),
        headers: headers(app, Some(key)),
        body: None,
    }
}

/// Read the answer to [`validate`].
pub fn parse_user(body: &[u8]) -> Result<User, Error> {
    let v: Value = serde_json::from_slice(body).map_err(|e| Error::Answer(e.to_string()))?;
    Ok(User {
        name: text(&v["name"]),
        premium: v["is_premium"].as_bool().unwrap_or(false),
    })
}

/// The download links of a file. Premium accounts need only the key; other accounts also need
/// the `key` and `expires` of an `nxm://` link from the website.
pub fn download_link(
    app: &App,
    key: &str,
    mod_id: u32,
    file_id: u32,
    link: Option<&crate::nxm::NxmLink>,
) -> Request {
    let mut url =
        format!("{API_V1}/games/{GAME_DOMAIN}/mods/{mod_id}/files/{file_id}/download_link.json");
    if let Some(l) = link {
        url.push_str(&format!("?key={}&expires={}", l.key, l.expires));
    }
    Request {
        method: "GET",
        url,
        headers: headers(app, Some(key)),
        body: None,
    }
}

/// Read the answer to [`download_link`]: the addresses to download from, the preferred first.
pub fn parse_download_links(body: &[u8]) -> Result<Vec<String>, Error> {
    let v: Value = serde_json::from_slice(body).map_err(|e| Error::Answer(e.to_string()))?;
    let links: Vec<String> = v
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| l["URI"].as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if links.is_empty() {
        return Err(Error::Answer("no download links".into()));
    }
    Ok(links)
}

/// The error for an answer with a status other than 2xx. A 403 from a download link means the
/// account needs Premium or a website link.
pub fn status_error(status: u16, body: &[u8], download: bool) -> Error {
    if download && status == 403 {
        return Error::NeedsPremium;
    }
    let message = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| String::from_utf8_lossy(body).chars().take(200).collect());
    if status == 401 {
        return Error::Status(401, format!("the API key is not valid ({message})"));
    }
    Error::Status(status, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::un_banished("0.2.0")
    }

    #[test]
    fn requests_name_the_application() {
        let r = validate(&app(), "KEY");
        assert_eq!(r.url, "https://api.nexusmods.com/v1/users/validate.json");
        assert!(r
            .headers
            .contains(&("Application-Name".into(), "Un-Banished".into())));
        assert!(r
            .headers
            .contains(&("Application-Version".into(), "0.2.0".into())));
        assert!(r.headers.contains(&("apikey".into(), "KEY".into())));
        // Searches need no key.
        let s = search(&app(), "terrain", Sort::Downloads, 0, 10);
        assert!(!s.headers.iter().any(|(k, _)| k == "apikey"));
        let body: Value = serde_json::from_str(s.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["variables"]["filter"]["name"][0]["value"], "terrain");
        assert_eq!(body["variables"]["count"], 10);
    }

    #[test]
    fn reads_a_search_answer() {
        let body = br#"{"data":{"mods":{"totalCount":3,"nodes":[{"modId":19,"name":"MoreTerrain",
            "summary":"More terrain.","version":"6","downloads":39616,"endorsements":965,
            "updatedAt":"2015-11-21T11:05:01Z","author":"cygnus54","thumbnailUrl":"https://x/t.jpg"}]}}}"#;
        let page = parse_search(body).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.mods[0].mod_id, 19);
        assert_eq!(page.mods[0].author, "cygnus54");
        assert_eq!(
            page.mods[0].page(),
            "https://www.nexusmods.com/banished/mods/19"
        );
        let err = parse_search(br#"{"errors":[{"message":"bad"}]}"#).unwrap_err();
        assert_eq!(err, Error::Answer("bad".into()));
    }

    #[test]
    fn picks_the_newest_main_file() {
        let f = |id, cat: &str, name: &str| ModFile {
            file_id: id,
            name: name.into(),
            file_name: format!("{name}.zip"),
            category: cat.into(),
            size: 1,
            version: String::new(),
        };
        let files = vec![
            f(1, "OLD_VERSION", "a"),
            f(3, "MAIN", "b"),
            f(5, "MAIN", "c"),
            f(6, "OPTIONAL", "d"),
        ];
        assert_eq!(main_file(&files).unwrap().file_id, 5);
        let files = vec![f(1, "OLD_VERSION", "a"), f(2, "UPDATE", "b")];
        assert_eq!(main_file(&files).unwrap().file_id, 2);
        assert_eq!(extension("CCNewFrontier155-27-1-55.RAR"), "rar");
    }

    #[test]
    fn download_links_and_errors() {
        let link = crate::nxm::NxmLink::parse(
            "nxm://banished/mods/19/files/49?key=abc&expires=1700000000&user_id=7",
        )
        .unwrap();
        let r = download_link(&app(), "KEY", 19, 49, Some(&link));
        assert!(r
            .url
            .ends_with("/mods/19/files/49/download_link.json?key=abc&expires=1700000000"));
        let links = parse_download_links(
            br#"[{"name":"Nexus CDN","short_name":"CDN","URI":"https://cf-files.nexusmods.com/x.zip"}]"#,
        )
        .unwrap();
        assert_eq!(links, vec!["https://cf-files.nexusmods.com/x.zip"]);
        assert_eq!(status_error(403, b"{}", true), Error::NeedsPremium);
        assert!(matches!(
            status_error(
                401,
                br#"{"message":"Please provide a valid API Key"}"#,
                false
            ),
            Error::Status(401, _)
        ));
    }
}

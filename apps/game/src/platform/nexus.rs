//! Nexus Mods for the Mods dialog's Browse tab: the API key, and the searches and installs run
//! in the background (a thread natively, `fetch` in the browser), whose replies the dialog
//! collects each frame with [`Nexus::poll`].
//!
//! Deviation: the original's Browse tab is the Steam Workshop browser (`FUN_14006f790`,
//! `FUN_14006dee0`); here it browses Nexus Mods, and only when the player has set an API key
//! (`NexusApiKey` in the settings, or `BANISHED_NEXUS_API_KEY` natively).

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

use banished_nexus::api::{self, ModSummary, SearchPage, Sort, User};
use banished_nexus::nxm::NxmLink;
use banished_nexus::{App, Error, Request};

/// The settings key (section and name) of the API key: `NexusApiKey=…`.
pub const KEY_SECTION: &str = "Nexus";
pub const KEY_NAME: &str = "ApiKey";

/// The settings key that records which Nexus mod a package came from:
/// `NexusMod_<file>=<mod id>`, for the Subscribed list.
pub const MOD_SECTION: &str = "NexusMod_";

/// Results per page. The Steam Workshop returns 50 per query (*assumed*: the original reads
/// the count from each answer, `+0x130`).
pub const PER_PAGE: u32 = 50;

/// Work for the background.
#[derive(Debug, Clone)]
pub enum Task {
    /// The account the key belongs to.
    Validate,
    /// A page of search results (`page` from 0).
    Search { text: String, sort: Sort, page: u32 },
    /// The mods with these ids (the Subscribed list).
    Installed(Vec<u32>),
    /// Install a mod's main file (Premium), or the file an `nxm://` link names.
    Install { mod_id: u32, link: Option<NxmLink> },
}

/// What came back.
#[derive(Debug, Clone)]
pub enum Reply {
    User(User),
    Results(SearchPage),
    /// Downloading: the bytes so far and the size when known.
    Progress(u64, Option<u64>),
    /// Installed: the mod and its packages (file name without `.pkm`, bytes).
    Installed(u32, Vec<(String, Vec<u8>)>),
    /// The download needs Premium or a website link: the mod.
    NeedsLink(u32),
    Failed(String),
}

/// The key, from the settings or (natively) the environment, which wins.
pub fn key(settings: &crate::settings::Persistence) -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(k) = std::env::var(banished_nexus::native::KEY_VARIABLE) {
        let k = k.trim().to_string();
        return (!k.is_empty()).then_some(k);
    }
    settings.get_text(KEY_SECTION, KEY_NAME)
}

fn app() -> App {
    App::un_banished(env!("CARGO_PKG_VERSION"))
}

/// The background client of the Browse tab.
pub struct Nexus {
    key: String,
    /// Where packages are written natively (the pack directory).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    dir: Option<std::path::PathBuf>,
    inbox: Inbox,
}

#[cfg(not(target_arch = "wasm32"))]
struct Inbox {
    send: std::sync::mpsc::Sender<Reply>,
    receive: std::sync::mpsc::Receiver<Reply>,
}

#[cfg(target_arch = "wasm32")]
struct Inbox(Rc<RefCell<Vec<Reply>>>);

impl Nexus {
    pub fn new(key: String, dir: Option<std::path::PathBuf>) -> Nexus {
        #[cfg(not(target_arch = "wasm32"))]
        let inbox = {
            let (send, receive) = std::sync::mpsc::channel();
            Inbox { send, receive }
        };
        #[cfg(target_arch = "wasm32")]
        let inbox = Inbox(Rc::new(RefCell::new(Vec::new())));
        Nexus { key, dir, inbox }
    }

    /// The replies that have arrived since the last call.
    pub fn poll(&self) -> Vec<Reply> {
        #[cfg(not(target_arch = "wasm32"))]
        return self.inbox.receive.try_iter().collect();
        #[cfg(target_arch = "wasm32")]
        return std::mem::take(&mut *self.inbox.0.borrow_mut());
    }

    /// Start `task` in the background.
    pub fn start(&self, task: Task) {
        let key = self.key.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let send = self.inbox.send.clone();
            let dir = self.dir.clone();
            std::thread::spawn(move || {
                let reply = native_task(&key, dir.as_deref(), task, &send);
                let _ = send.send(reply);
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let inbox = self.inbox.0.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let reply = web_task(&key, task, &inbox).await;
                inbox.borrow_mut().push(reply);
            });
        }
    }
}

fn failed(e: Error) -> Reply {
    Reply::Failed(e.to_string())
}

/// The reply to a finished request, for the tasks that are one request each.
fn answer(task: &Task, body: Result<Vec<u8>, Error>) -> Reply {
    let body = match body {
        Ok(b) => b,
        Err(e) => return failed(e),
    };
    let result = match task {
        Task::Validate => api::parse_user(&body).map(Reply::User),
        Task::Search { .. } => api::parse_search(&body).map(Reply::Results),
        Task::Installed(ids) => api::parse_by_ids(&body, ids).map(|mods: Vec<ModSummary>| {
            Reply::Results(SearchPage {
                total: mods.len() as u32,
                mods,
            })
        }),
        Task::Install { .. } => unreachable!("installs are several requests"),
    };
    result.unwrap_or_else(failed)
}

fn request(key: &str, task: &Task) -> Request {
    let app = app();
    match task {
        Task::Validate => api::validate(&app, key),
        Task::Search { text, sort, page } => {
            api::search(&app, text, *sort, page * PER_PAGE, PER_PAGE)
        }
        Task::Installed(ids) => api::by_ids(&app, ids),
        Task::Install { .. } => unreachable!("installs are several requests"),
    }
}

/// The packages of an install as (file name without `.pkm`, bytes).
fn stems(packages: Vec<banished_nexus::install::Package>) -> Vec<(String, Vec<u8>)> {
    packages
        .into_iter()
        .map(|p| (p.name[..p.name.len() - 4].to_string(), p.data))
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn native_task(
    key: &str,
    dir: Option<&std::path::Path>,
    task: Task,
    send: &std::sync::mpsc::Sender<Reply>,
) -> Reply {
    use banished_nexus::native;
    let Task::Install { mod_id, link } = &task else {
        return answer(&task, native::send(&request(key, &task), false));
    };
    let app = app();
    let run = || -> Result<Vec<(String, Vec<u8>)>, Error> {
        let file_id = match link {
            Some(l) => l.file_id,
            None => {
                let files = api::parse_files(&native::send(&api::files(&app, *mod_id), false)?)?;
                api::main_file(&files).ok_or(Error::NoPackage)?.file_id
            }
        };
        let request = api::download_link(&app, key, *mod_id, file_id, link.as_ref());
        let links = api::parse_download_links(&native::send(&request, true)?)?;
        let data = native::download(&links[0], |done, total| {
            let _ = send.send(Reply::Progress(done, total));
        })?;
        let packages = banished_nexus::install::packages(&file_name(&links[0]), &data)?;
        if let Some(dir) = dir {
            native::install(dir, &packages)?;
        }
        Ok(stems(packages))
    };
    match run() {
        Ok(found) => Reply::Installed(*mod_id, found),
        Err(Error::NeedsPremium) => Reply::NeedsLink(*mod_id),
        Err(e) => failed(e),
    }
}

/// A download's file name: the last part of its address, without the query.
fn file_name(url: &str) -> String {
    url.split('?')
        .next()
        .and_then(|u| u.rsplit('/').next())
        .map(|n| n.replace("%20", " "))
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
async fn fetch(request: &Request, download: bool) -> Result<Vec<u8>, Error> {
    use wasm_bindgen::JsCast;
    let net = |e: wasm_bindgen::JsValue| Error::Network(format!("{e:?}"));
    let init = web_sys::RequestInit::new();
    init.set_method(request.method);
    init.set_mode(web_sys::RequestMode::Cors);
    let headers = web_sys::Headers::new().map_err(net)?;
    for (k, v) in &request.headers {
        headers.set(k, v).map_err(net)?;
    }
    init.set_headers(&headers);
    if let Some(body) = &request.body {
        init.set_body(&wasm_bindgen::JsValue::from_str(body));
    }
    let req = web_sys::Request::new_with_str_and_init(&request.url, &init).map_err(net)?;
    let window = web_sys::window().ok_or_else(|| Error::Network("no window".into()))?;
    let response: web_sys::Response =
        wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&req))
            .await
            .map_err(net)?
            .dyn_into()
            .map_err(net)?;
    let buffer = wasm_bindgen_futures::JsFuture::from(response.array_buffer().map_err(net)?)
        .await
        .map_err(net)?;
    let body = js_sys::Uint8Array::new(&buffer).to_vec();
    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(api::status_error(status, &body, download));
    }
    Ok(body)
}

#[cfg(target_arch = "wasm32")]
async fn web_task(key: &str, task: Task, inbox: &Rc<RefCell<Vec<Reply>>>) -> Reply {
    let Task::Install { mod_id, link } = &task else {
        return answer(&task, fetch(&request(key, &task), false).await);
    };
    let app = app();
    let run = async {
        let file_id = match link {
            Some(l) => l.file_id,
            None => {
                let files = api::parse_files(&fetch(&api::files(&app, *mod_id), false).await?)?;
                api::main_file(&files).ok_or(Error::NoPackage)?.file_id
            }
        };
        let request = api::download_link(&app, key, *mod_id, file_id, link.as_ref());
        let links = api::parse_download_links(&fetch(&request, true).await?)?;
        inbox.borrow_mut().push(Reply::Progress(0, None));
        let download = Request {
            method: "GET",
            url: links[0].clone(),
            headers: Vec::new(),
            body: None,
        };
        let data = fetch(&download, false).await?;
        let packages = banished_nexus::install::packages(&file_name(&links[0]), &data)?;
        // The browser's way of copying them into `WinData`, as for dropped mods.
        for p in &packages {
            crate::saves::save_store::store_mod(&p.name[..p.name.len() - 4], &p.data);
        }
        Ok::<_, Error>(stems(packages))
    };
    match run.await {
        Ok(found) => Reply::Installed(*mod_id, found),
        Err(Error::NeedsPremium) => Reply::NeedsLink(*mod_id),
        Err(e) => failed(e),
    }
}

/// Open an address in the player's web browser.
pub fn open_in_browser(url: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(url, "_blank");
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let result = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(url).spawn()
        } else if cfg!(target_os = "windows") {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", url])
                .spawn()
        } else {
            std::process::Command::new("xdg-open").arg(url).spawn()
        };
        if let Err(e) = result {
            log::warn!("could not open {url}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn download_file_names() {
        assert_eq!(
            super::file_name("https://cf-files.nexusmods.com/cdn/617/19/More%20Terrain%20v6-19-6.zip?md5=x&expires=1"),
            "More Terrain v6-19-6.zip"
        );
    }
}

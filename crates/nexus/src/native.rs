//! Sending requests and downloading files on native builds (blocking; run it off the main
//! thread in the game), and writing installed packages to the pack folder.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{install::Package, Error, Request};

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .build()
        .into()
}

/// Send a request and return the body of a 2xx answer. `download` marks a download-link
/// request, whose 403 means the account needs Premium.
pub fn send(request: &Request, download: bool) -> Result<Vec<u8>, Error> {
    let agent = agent();
    let result = match request.method {
        "POST" => {
            let mut r = agent.post(&request.url);
            for (k, v) in &request.headers {
                r = r.header(k, v);
            }
            r.send(request.body.clone().unwrap_or_default())
        }
        _ => {
            let mut r = agent.get(&request.url);
            for (k, v) in &request.headers {
                r = r.header(k, v);
            }
            r.call()
        }
    };
    let mut response = result.map_err(|e| Error::Network(e.to_string()))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(16 << 20)
        .read_to_vec()
        .map_err(|e| Error::Network(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(crate::api::status_error(status, &body, download));
    }
    Ok(body)
}

/// Download a file, calling `progress(done, total)` as it arrives (`total` when the server
/// says it). Download links need no headers.
pub fn download(url: &str, mut progress: impl FnMut(u64, Option<u64>)) -> Result<Vec<u8>, Error> {
    let mut response = agent()
        .get(url)
        .call()
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(Error::Status(status, "the download failed".into()));
    }
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(crate::install::MAX_PACKAGE * 2)
        .reader();
    let mut data = Vec::with_capacity(total.unwrap_or(0).min(256 << 20) as usize);
    let mut chunk = vec![0; 64 << 10];
    loop {
        let n = reader
            .read(&mut chunk)
            .map_err(|e| Error::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..n]);
        progress(data.len() as u64, total);
    }
    Ok(data)
}

/// Write packages into the pack folder (`WinData`), replacing packages of the same name. Each
/// is written to a temporary file first and then renamed, so a failed write leaves no
/// half-written package.
pub fn install(dir: &Path, packages: &[Package]) -> Result<Vec<PathBuf>, Error> {
    let mut written = Vec::new();
    for p in packages {
        let path = dir.join(&p.name);
        let temp = dir.join(format!(".{}.download", p.name));
        let io = |e: std::io::Error| Error::Network(format!("could not write {}: {e}", p.name));
        std::fs::write(&temp, &p.data).map_err(io)?;
        std::fs::rename(&temp, &path).map_err(io)?;
        written.push(path);
    }
    Ok(written)
}

/// What to install: a file of a mod (Premium), or the file an `nxm://` link names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    File {
        mod_id: u32,
        file_id: u32,
        file_name: String,
    },
    Link(crate::nxm::NxmLink),
}

/// How far an installation has got, for a progress display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Asking Nexus Mods for the download link.
    Link,
    /// Downloading: bytes so far, and the size when known.
    Download(u64, Option<u64>),
    /// Unpacking and writing the packages.
    Install,
}

/// Get the download link for `source`, download it, take out its `.pkm` packages and write
/// them to `dir`. Returns the paths written.
pub fn install_from(
    app: &crate::App,
    key: &str,
    source: &Source,
    dir: &Path,
    mut progress: impl FnMut(Step),
) -> Result<Vec<PathBuf>, Error> {
    progress(Step::Link);
    let (mod_id, file_id, link) = match source {
        Source::File {
            mod_id, file_id, ..
        } => (*mod_id, *file_id, None),
        Source::Link(l) => (l.mod_id, l.file_id, Some(l)),
    };
    let request = crate::api::download_link(app, key, mod_id, file_id, link);
    let links = crate::api::parse_download_links(&send(&request, true)?)?;
    let url = &links[0];
    let data = download(url, |done, total| progress(Step::Download(done, total)))?;
    progress(Step::Install);
    // The file name is the last part of the download address, without its query.
    let file_name = match source {
        Source::File { file_name, .. } if !file_name.is_empty() => file_name.clone(),
        _ => url
            .split('?')
            .next()
            .and_then(|u| u.rsplit('/').next())
            .map(|n| n.replace("%20", " "))
            .unwrap_or_default(),
    };
    let packages = crate::install::packages(&file_name, &data)?;
    install(dir, &packages)
}

/// The environment variable that holds an API key; it wins over the settings file.
pub const KEY_VARIABLE: &str = "BANISHED_NEXUS_API_KEY";

/// The settings file's key for the API key (the file is `key=value` lines).
pub const KEY_SETTING: &str = "NexusApiKey";

/// The game's settings file: `BANISHED_SETTINGS` if set, else `settings.txt` in the
/// `banished-wasm` folder of the platform's configuration folder (as the game reads it).
pub fn settings_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BANISHED_SETTINGS") {
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

/// The API key: [`KEY_VARIABLE`], else the [`KEY_SETTING`] line of the settings file. An
/// empty value counts as none.
pub fn stored_key() -> Option<String> {
    if let Ok(k) = std::env::var(KEY_VARIABLE) {
        let k = k.trim();
        return (!k.is_empty()).then(|| k.to_string());
    }
    let text = std::fs::read_to_string(settings_path()?).ok()?;
    key_in_settings(&text)
}

/// The [`KEY_SETTING`] value in a settings file's text.
pub fn key_in_settings(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == KEY_SETTING)
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_key_from_settings() {
        let text = "VideoVSync=true\nNexusApiKey= abc123 \n";
        assert_eq!(super::key_in_settings(text).as_deref(), Some("abc123"));
        assert_eq!(super::key_in_settings("NexusApiKey=\n"), None);
    }
}

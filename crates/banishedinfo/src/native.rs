//! Sending the requests on native builds (blocking; run them off the main thread in a window),
//! and installing a version.

use std::io::Read;
use std::path::{Path, PathBuf};

use ureq::ResponseExt;

use crate::{Error, Mod, Sort, Version};

/// A client that names the application in its user agent.
pub struct Client {
    agent: ureq::Agent,
    user_agent: String,
}

impl Client {
    /// A client for Un-Banished at `version`.
    pub fn new(version: &str) -> Client {
        Client {
            agent: ureq::Agent::config_builder()
                .http_status_as_error(false)
                .timeout_global(Some(std::time::Duration::from_secs(600)))
                .build()
                .into(),
            user_agent: crate::user_agent(version),
        }
    }

    /// GET an address; returns the final address (after redirects) and the body.
    fn get(
        &self,
        url: &str,
        limit: u64,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<(String, Vec<u8>), Error> {
        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", &self.user_agent)
            .call()
            .map_err(|e| Error::Network(e.to_string()))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(Error::Status(status));
        }
        let final_url = response.get_uri().to_string();
        let total = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        let mut reader = response.body_mut().with_config().limit(limit).reader();
        let mut data = Vec::new();
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
        Ok((final_url, data))
    }

    /// A small file from the site, such as a thumbnail (at most 4 MB).
    pub fn image(&self, url: &str) -> Result<Vec<u8>, Error> {
        self.get(url, 4 << 20, |_, _| {}).map(|(_, b)| b)
    }

    fn page(&self, url: &str) -> Result<String, Error> {
        let (_, body) = self.get(url, 16 << 20, |_, _| {})?;
        String::from_utf8(body).map_err(|e| Error::Answer(e.to_string()))
    }

    /// `count` mods from `start` of a section (`""` for all), in an order.
    pub fn list(
        &self,
        sort: Sort,
        section: &str,
        start: u32,
        count: u32,
    ) -> Result<Vec<Mod>, Error> {
        let (_, body) = self.get(
            &crate::list_url(sort, section, start, count),
            16 << 20,
            |_, _| {},
        )?;
        crate::parse_list(&body)
    }

    /// The mods the site's search finds for `text`.
    pub fn search(&self, text: &str) -> Result<Vec<Mod>, Error> {
        crate::parse_search(&self.page(&crate::search_url(text))?)
    }

    /// A mod's versions, newest first.
    pub fn versions(&self, m: &Mod) -> Result<Vec<Version>, Error> {
        let versions = crate::parse_versions(&self.page(&m.page())?, m.id);
        if versions.is_empty() {
            return Err(Error::Answer(format!("no downloads on {}", m.page())));
        }
        Ok(versions)
    }

    /// Download a version: its file name and bytes.
    pub fn download(
        &self,
        version: &Version,
        progress: impl FnMut(u64, Option<u64>),
    ) -> Result<(String, Vec<u8>), Error> {
        let limit = unbanished_nexus::install::MAX_PACKAGE * 2;
        let (final_url, data) = self.get(&version.file, limit, progress)?;
        Ok((crate::file_name(&final_url), data))
    }

    /// Install a version into the pack folder: the `.pkm` packages of the download, written
    /// through a temporary file each. Returns the paths written.
    pub fn install(
        &self,
        version: &Version,
        dir: &Path,
        progress: impl FnMut(u64, Option<u64>),
    ) -> Result<Vec<PathBuf>, Error> {
        let (name, data) = self.download(version, progress)?;
        let packages = unbanished_nexus::install::packages(&name, &data)
            .map_err(|e| Error::Install(e.to_string()))?;
        unbanished_nexus::native::install(dir, &packages).map_err(|e| Error::Install(e.to_string()))
    }
}

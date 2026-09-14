use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use directories::BaseDirs;
use fs2::FileExt;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use tempfile::NamedTempFile;

const MAX_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024;
const REQUEST_INTERVAL: Duration = Duration::from_secs(3);

pub struct WebClient {
    client: Client,
    pub cache_dir: PathBuf,
    pub refresh: bool,
    pub offline: bool,
}

impl WebClient {
    pub fn new(cache_dir: Option<PathBuf>, refresh: bool, offline: bool) -> Result<Self> {
        let cache_dir = cache_dir
            .or_else(|| std::env::var_os("ARXIV_CACHE_DIR").map(PathBuf::from))
            .or_else(|| BaseDirs::new().map(|base| base.cache_dir().join("arxiv")))
            .context("cannot locate cache directory; set --cache-dir")?;
        fs::create_dir_all(&cache_dir)?;
        let client = Client::builder()
            .user_agent(concat!("arxiv-cli/", env!("CARGO_PKG_VERSION"), " (+https://github.com/1jmdev/arxiv)"))
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    attempt.error("too many redirects")
                } else if attempt.url().host_str().is_some_and(|host| {
                    host == "arxiv.org" || host.ends_with(".arxiv.org")
                }) {
                    attempt.follow()
                } else {
                    attempt.error("refusing redirect outside arxiv.org")
                }
            }))
            .build()?;
        Ok(Self { client, cache_dir, refresh, offline })
    }

    pub fn resource(
        &self,
        url: &str,
        relative_path: &Path,
        maximum_age: Option<Duration>,
    ) -> Result<PathBuf> {
        let path = self.cache_dir.join(relative_path);
        if path.is_file() && !self.refresh {
            let fresh = maximum_age.is_none_or(|age| {
                fs::metadata(&path)
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| modified.elapsed().ok())
                    .is_some_and(|elapsed| elapsed < age)
            });
            if fresh || self.offline {
                return Ok(path);
            }
        }
        ensure!(!self.offline, "resource is not cached: {url}");
        let bytes = self.fetch(url)?;
        Self::write_atomic(&path, &bytes)?;
        Ok(path)
    }

    pub fn fetch(&self, url: &str) -> Result<Vec<u8>> {
        ensure!(!self.offline, "network disabled by --offline");
        let parsed = reqwest::Url::parse(url)?;
        ensure!(
            parsed.scheme() == "https" && parsed.host_str() == Some("arxiv.org"),
            "only direct HTTPS requests to arxiv.org are supported"
        );
        for attempt in 0..3 {
            let request_lock = self.acquire_request_lock()?;
            let response = self.client.get(url).send().with_context(|| format!("fetching {url}"))?;
            let status = response.status();
            if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                let delay = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(3 * (attempt + 1));
                drop(response);
                drop(request_lock);
                ensure!(attempt < 2, "arXiv returned {status} for {url}; try again later");
                ensure!(delay <= 60, "arXiv requests a {delay}-second delay; try again later");
                thread::sleep(Duration::from_secs(delay));
                continue;
            }
            ensure!(status.is_success(), "arXiv returned {status} for {url}");
            ensure!(
                response.content_length().is_none_or(|size| size <= MAX_DOWNLOAD_BYTES),
                "download exceeds the 100 MiB limit"
            );
            let mut bytes = Vec::new();
            response.take(MAX_DOWNLOAD_BYTES + 1).read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= MAX_DOWNLOAD_BYTES, "download exceeds the 100 MiB limit");
            drop(request_lock);
            return Ok(bytes);
        }
        bail!("request retries exhausted")
    }

    fn acquire_request_lock(&self) -> Result<File> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.cache_dir.join("request.lock"))?;
        file.lock_exclusive()?;
        let mut timestamp = String::new();
        file.read_to_string(&mut timestamp)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        if let Ok(previous) = timestamp.parse::<u64>() {
            let elapsed = now.saturating_sub(Duration::from_millis(previous));
            if elapsed < REQUEST_INTERVAL {
                thread::sleep(REQUEST_INTERVAL - elapsed);
            }
        }
        file.set_len(0)?;
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(0))?;
        write!(file, "{}", SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        file.flush()?;
        Ok(file)
    }

    pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
        let parent = path.parent().context("cache path has no parent")?;
        fs::create_dir_all(parent)?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        temporary.write_all(bytes)?;
        temporary.persist(path).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}

//! HTTP for every online feature: one client, JSON fetches with retries, and
//! verified downloads that never leave a half-written file behind.
//!
//! Only idempotent requests are retried (GETs, on connection errors, 429,
//! 5xx, and a body cut off partway). Timeouts limit how long a connection
//! may stall, not how long a big download may take.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt as _;
use reqwest::StatusCode;
use reqwest::redirect::Policy;
use serde::de::DeserializeOwned;
use tokio::io::AsyncWriteExt as _;

use crate::fsutil;

/// Sent with every request so services can tell who's calling.
pub const USER_AGENT: &str = concat!(
    "mctui/",
    env!("CARGO_PKG_VERSION"),
    " (github.com/aayushdutt/mctui)"
);

const ATTEMPTS: u32 = 3;
const MAX_RETRY_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{url} answered {status}")]
    Status { url: String, status: StatusCode },
    #[error("couldn't reach {url}: {source}")]
    Request {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("unexpected response from {url}: {source}")]
    Decode {
        url: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{url} downloaded with the wrong checksum (expected {expected}, got {actual})")]
    Checksum {
        url: String,
        expected: String,
        actual: String,
    },
    #[error("couldn't save {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    /// Whether the service said the request itself was bad or unauthorized
    /// (as opposed to being unreachable or broken).
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Error::Status { status, .. } => Some(*status),
            _ => None,
        }
    }

    fn io(path: &Path, source: std::io::Error) -> Self {
        Error::Io {
            path: path.to_owned(),
            source,
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A published checksum, as lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hash {
    Sha1(String),
    Sha256(String),
}

impl Hash {
    fn expected(&self) -> &str {
        match self {
            Hash::Sha1(h) | Hash::Sha256(h) => h,
        }
    }

    fn hasher(&self) -> Hasher {
        match self {
            Hash::Sha1(_) => Hasher::Sha1(sha1::Sha1::default()),
            Hash::Sha256(_) => Hasher::Sha256(sha2::Sha256::default()),
        }
    }

    /// Whether `bytes` have this checksum.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        let mut hasher = self.hasher();
        hasher.update(bytes);
        hasher.finish().eq_ignore_ascii_case(self.expected())
    }

    /// Whether the file at `path` already has this checksum.
    pub async fn matches_file(&self, path: &Path) -> bool {
        let (path, sha256) = (path.to_owned(), matches!(self, Hash::Sha256(_)));
        let actual = fsutil::blocking(move || {
            if sha256 {
                fsutil::hash_file::<sha2::Sha256>(&path)
            } else {
                fsutil::hash_file::<sha1::Sha1>(&path)
            }
        })
        .await;
        actual.is_ok_and(|a| a.eq_ignore_ascii_case(self.expected()))
    }
}

enum Hasher {
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
}

impl Hasher {
    fn update(&mut self, data: &[u8]) {
        use sha2::Digest as _;
        match self {
            Hasher::Sha1(h) => h.update(data),
            Hasher::Sha256(h) => h.update(data),
        }
    }

    fn finish(self) -> String {
        use sha2::Digest as _;
        match self {
            Hasher::Sha1(h) => fsutil::hex(&h.finalize()),
            Hasher::Sha256(h) => fsutil::hex(&h.finalize()),
        }
    }
}

/// A file to fetch: where from, where to, and what it should hash to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub url: String,
    pub dest: PathBuf,
    /// The published checksum. Without one, the file is always fetched.
    pub hash: Option<Hash>,
    /// Expected size in bytes, for progress before the response arrives.
    pub size: Option<u64>,
}

/// Progress of a batch of downloads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    /// Sum of known sizes; may be less than the real total.
    pub bytes_total: u64,
}

/// `link` as a URL safe to open in a browser: an absolute `http` or `https`
/// URL with a host, normalised (spaces and the like escaped). `None` for
/// anything else a service might send, such as `file:` or `javascript:`
/// links, or a path the system's opener would run.
pub fn web_url(link: &str) -> Option<String> {
    let url = reqwest::Url::parse(link).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.has_host()).then(|| url.into())
}

/// The shared HTTP client. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Http {
    client: reqwest::Client,
    /// The same, but it never follows a redirect.
    no_redirects: reqwest::Client,
}

impl Http {
    pub fn new() -> Result<Self> {
        let build = |redirects| {
            reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(30))
                .redirect(redirects)
                .build()
                .map_err(|source| Error::Request {
                    url: String::new(),
                    source,
                })
        };
        Ok(Self {
            client: build(Policy::default())?,
            no_redirects: build(Policy::none())?,
        })
    }

    /// The underlying client, for requests this module doesn't wrap (POSTs).
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// A client that never follows redirects, for requests carrying tokens:
    /// on a 307 or 308, reqwest sends a request's body again, to whatever
    /// host the redirect names.
    pub fn client_without_redirects(&self) -> &reqwest::Client {
        &self.no_redirects
    }

    /// GETs `url` and decodes JSON, retrying transient failures.
    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let bytes = self.get_bytes(url).await?;
        serde_json::from_slice(&bytes).map_err(|source| Error::Decode {
            url: url.to_owned(),
            source,
        })
    }

    /// GETs `url` into memory, retrying transient failures, a body cut off
    /// partway included.
    pub async fn get_bytes(&self, url: &str) -> Result<bytes::Bytes> {
        let mut retry = Retry::new(url);
        loop {
            let failure = match self.get_once(url).await {
                Ok(response) => match response.bytes().await {
                    Ok(bytes) => return Ok(bytes),
                    Err(source) => Failure::cut_off(url, source),
                },
                Err(failure) => failure,
            };
            retry.after(failure).await?;
        }
    }

    /// Sends a GET, retrying connection errors, 429 and 5xx with backoff
    /// (honouring a short `Retry-After`). Returns a successful response.
    pub async fn get(&self, url: &str) -> Result<reqwest::Response> {
        let mut retry = Retry::new(url);
        loop {
            match self.get_once(url).await {
                Ok(response) => return Ok(response),
                Err(failure) => retry.after(failure).await?,
            }
        }
    }

    /// Sends a GET once. Returns a successful response.
    async fn get_once(&self, url: &str) -> Result<reqwest::Response, Failure> {
        match self.client.get(url).send().await {
            Ok(response) if response.status().is_success() => Ok(response),
            Ok(response) => {
                let status = response.status();
                let error = Error::Status {
                    url: url.to_owned(),
                    status,
                };
                Err(if retryable(status) {
                    Failure::Transient {
                        error,
                        wait: retry_after(&response),
                    }
                } else {
                    Failure::Final(error)
                })
            }
            Err(source) => {
                let transient = source.is_connect() || source.is_timeout();
                let error = Error::Request {
                    url: url.to_owned(),
                    source,
                };
                Err(if transient {
                    Failure::Transient { error, wait: None }
                } else {
                    Failure::Final(error)
                })
            }
        }
    }

    /// Downloads `job.url` to `job.dest`, verifying the checksum. Skips the
    /// download when `dest` already has the right checksum. Writes to a temp
    /// file beside `dest` and renames it into place, so an interrupted or
    /// cancelled download never leaves a partial file. A download that fails
    /// partway is started again, as a failed request is retried. `on_bytes`
    /// gets each chunk's length as it arrives; a retry counts only what goes
    /// past what the failed try reported.
    pub async fn download(&self, job: &Download, mut on_bytes: impl FnMut(u64)) -> Result<()> {
        if let Some(hash) = &job.hash
            && hash.matches_file(&job.dest).await
        {
            on_bytes(job.size.unwrap_or(0));
            return Ok(());
        }
        let dir = job.dest.parent().unwrap_or(Path::new("."));
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| Error::io(dir, e))?;

        let mut retry = Retry::new(&job.url);
        let mut reported = 0;
        let temp = loop {
            match self
                .fetch_once(job, dir, &mut reported, &mut on_bytes)
                .await
            {
                Ok(temp) => break temp,
                Err(failure) => retry.after(failure).await?,
            }
        };
        let dest = job.dest.clone();
        fsutil::blocking(move || temp.persist(&dest).map(drop).map_err(|e| e.error))
            .await
            .map_err(|e| Error::io(&job.dest, e))
    }

    /// One try at `job`: fetches it into a temp file in `dir` and checks its
    /// checksum. Reports only the bytes past `reported`, and moves that on.
    async fn fetch_once(
        &self,
        job: &Download,
        dir: &Path,
        reported: &mut u64,
        on_bytes: &mut impl FnMut(u64),
    ) -> Result<tempfile::NamedTempFile, Failure> {
        let response = self.get_once(&job.url).await?;
        let temp_dir = dir.to_owned();
        let (temp, file) = fsutil::blocking(move || {
            let temp = tempfile::NamedTempFile::new_in(temp_dir)?;
            let file = temp.reopen()?;
            Ok::<_, io::Error>((temp, file))
        })
        .await
        .map_err(|e| Error::io(dir, e))?;
        let mut file = tokio::fs::File::from_std(file);

        let mut hasher = job.hash.as_ref().map(Hash::hasher);
        let mut received = 0;
        let mut body = response.bytes_stream();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|source| Failure::cut_off(&job.url, source))?;
            if let Some(h) = &mut hasher {
                h.update(&chunk);
            }
            file.write_all(&chunk)
                .await
                .map_err(|e| Error::io(temp.path(), e))?;
            received += chunk.len() as u64;
            if received > *reported {
                on_bytes(received - *reported);
                *reported = received;
            }
        }
        // Finishes the last write; no fsync. The checksum and the rename keep
        // the file whole, and a full flush per file (F_FULLFSYNC on macOS)
        // would add seconds to a first launch's thousands of assets.
        file.flush().await.map_err(|e| Error::io(temp.path(), e))?;
        drop(file);

        if let (Some(hash), Some(hasher)) = (&job.hash, hasher) {
            let actual = hasher.finish();
            if !actual.eq_ignore_ascii_case(hash.expected()) {
                return Err(Failure::Final(Error::Checksum {
                    url: job.url.clone(),
                    expected: hash.expected().to_owned(),
                    actual,
                }));
            }
        }
        Ok(temp)
    }

    /// Downloads every job, `concurrency` at a time, reporting progress after
    /// each chunk. Jobs sharing a destination are fetched once. Stops at the
    /// first failure.
    pub async fn download_all(
        &self,
        mut jobs: Vec<Download>,
        concurrency: usize,
        on_progress: impl Fn(Progress) + Sync,
    ) -> Result<()> {
        jobs.sort_by(|a, b| a.dest.cmp(&b.dest));
        jobs.dedup_by(|a, b| a.dest == b.dest);
        let progress = std::sync::Mutex::new(Progress {
            files_total: jobs.len(),
            bytes_total: jobs.iter().filter_map(|j| j.size).sum(),
            ..Progress::default()
        });
        let report = |bytes: u64, file_done: bool| {
            let snapshot = {
                let mut p = progress
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                p.bytes_done += bytes;
                p.files_done += usize::from(file_done);
                *p
            };
            on_progress(snapshot);
        };
        // Built up front (not in a `map` closure over the stream) so the
        // future stays `Send` and can run on a spawned task.
        let downloads: Vec<_> = jobs
            .iter()
            .map(|job| self.download(job, |n| report(n, false)))
            .collect();
        let mut pending =
            futures_util::stream::iter(downloads).buffer_unordered(concurrency.max(1));
        while let Some(result) = pending.next().await {
            result?;
            report(0, true);
        }
        Ok(())
    }
}

/// Why one try at a request failed.
enum Failure {
    /// Worth another try, after `wait` if the service asked for one.
    Transient {
        error: Error,
        wait: Option<Duration>,
    },
    /// Trying again won't help.
    Final(Error),
}

impl Failure {
    /// The response body stopped partway: a dropped connection, most likely.
    fn cut_off(url: &str, source: reqwest::Error) -> Self {
        Failure::Transient {
            error: Error::Request {
                url: url.to_owned(),
                source,
            },
            wait: None,
        }
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Failure::Final(error)
    }
}

/// Counts the tries at one request, and waits between them.
struct Retry<'a> {
    url: &'a str,
    attempt: u32,
}

impl<'a> Retry<'a> {
    fn new(url: &'a str) -> Self {
        Self { url, attempt: 1 }
    }

    /// After a failed try: waits with backoff (or as long as the service
    /// asked, up to a limit) if the failure is worth another and tries are
    /// left; else gives up with its error.
    async fn after(&mut self, failure: Failure) -> Result<()> {
        match failure {
            Failure::Transient { error, wait } if self.attempt < ATTEMPTS => {
                let wait = wait.unwrap_or_else(|| backoff(self.attempt));
                tracing::debug!(url = self.url, attempt = self.attempt, ?wait, %error, "retrying");
                tokio::time::sleep(wait.min(MAX_RETRY_WAIT)).await;
                self.attempt += 1;
                Ok(())
            }
            Failure::Transient { error, .. } | Failure::Final(error) => Err(error),
        }
    }
}

fn retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500 * 2u64.pow(attempt - 1))
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    let secs = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .parse()
        .ok()?;
    Some(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const HELLO_SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

    async fn server_with(route: &str, response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn get_json_decodes_and_reports_bad_bodies() {
        let server = server_with(
            "/v",
            ResponseTemplate::new(200).set_body_string(r#"{"a":1}"#),
        )
        .await;
        let http = Http::new().unwrap();
        let v: serde_json::Value = http.get_json(&format!("{}/v", server.uri())).await.unwrap();
        assert_eq!(v["a"], 1);

        let server = server_with("/v", ResponseTemplate::new(200).set_body_string("nope")).await;
        let err = http
            .get_json::<serde_json::Value>(&format!("{}/v", server.uri()))
            .await;
        assert!(matches!(err, Err(Error::Decode { .. })));
    }

    #[tokio::test]
    async fn retries_server_errors_then_gives_up() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let http = Http::new().unwrap();
        let body = http.get_bytes(&server.uri()).await.unwrap();
        assert_eq!(&body[..], b"ok");

        // 404 is final: no retry, a Status error.
        let server = server_with("/", ResponseTemplate::new(404)).await;
        let err = http.get_bytes(&server.uri()).await.unwrap_err();
        assert_eq!(err.status(), Some(StatusCode::NOT_FOUND));
        assert_eq!(
            server.received_requests().await.unwrap_or_default().len(),
            1
        );
    }

    #[tokio::test]
    async fn download_verifies_and_skips_what_is_already_there() {
        let server = server_with("/f", ResponseTemplate::new(200).set_body_string("hello")).await;
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new().unwrap();
        let job = Download {
            url: format!("{}/f", server.uri()),
            dest: dir.path().join("sub/f.txt"),
            hash: Some(Hash::Sha1(HELLO_SHA1.into())),
            size: Some(5),
        };
        let mut seen = 0;
        http.download(&job, |n| seen += n).await.unwrap();
        assert_eq!(std::fs::read_to_string(&job.dest).unwrap(), "hello");
        assert_eq!(seen, 5);

        // Already there with the right hash: no second request.
        http.download(&job, |_| {}).await.unwrap();
        assert_eq!(
            server.received_requests().await.unwrap_or_default().len(),
            1
        );
    }

    #[tokio::test]
    async fn a_bad_checksum_leaves_nothing_behind() {
        let server = server_with("/f", ResponseTemplate::new(200).set_body_string("evil")).await;
        let dir = tempfile::tempdir().unwrap();
        let job = Download {
            url: format!("{}/f", server.uri()),
            dest: dir.path().join("f.txt"),
            hash: Some(Hash::Sha1(HELLO_SHA1.into())),
            size: None,
        };
        let err = Http::new()
            .unwrap()
            .download(&job, |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Checksum { .. }));
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "no temp or partial file"
        );
    }

    #[tokio::test]
    async fn download_all_dedups_and_counts() {
        let server = server_with("/f", ResponseTemplate::new(200).set_body_string("hello")).await;
        let dir = tempfile::tempdir().unwrap();
        let job = |name: &str| Download {
            url: format!("{}/f", server.uri()),
            dest: dir.path().join(name),
            hash: Some(Hash::Sha1(HELLO_SHA1.into())),
            size: Some(5),
        };
        let last = std::sync::Mutex::new(Progress::default());
        Http::new()
            .unwrap()
            .download_all(vec![job("a"), job("b"), job("a")], 4, |p| {
                *last.lock().unwrap() = p;
            })
            .await
            .unwrap();
        let last = *last.lock().unwrap();
        assert_eq!((last.files_done, last.files_total), (2, 2));
        assert_eq!((last.bytes_done, last.bytes_total), (10, 10));
    }

    /// A server whose first answer is cut off after `first` bytes of
    /// `body`, and whose later answers are whole. Returns its address.
    async fn flaky_server(body: &'static [u8], first: usize) -> std::net::SocketAddr {
        use tokio::io::AsyncReadExt as _;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut cut = true;
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buf = [0; 1024];
                while !request.ends_with(b"\r\n\r\n") {
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buf[..n]);
                }
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let sent = if cut { &body[..first] } else { body };
                cut = false;
                socket.write_all(head.as_bytes()).await.unwrap();
                socket.write_all(sent).await.unwrap();
                // Dropping the socket closes the connection, partway or not.
            }
        });
        addr
    }

    #[tokio::test]
    async fn a_download_cut_off_partway_is_tried_again() {
        let addr = flaky_server(b"hello", 2).await;
        let dir = tempfile::tempdir().unwrap();
        let job = Download {
            url: format!("http://{addr}/f"),
            dest: dir.path().join("f.txt"),
            hash: Some(Hash::Sha1(HELLO_SHA1.into())),
            size: Some(5),
        };
        let mut seen = 0;
        Http::new()
            .unwrap()
            .download(&job, |n| seen += n)
            .await
            .unwrap();
        assert_eq!(std::fs::read_to_string(&job.dest).unwrap(), "hello");
        assert_eq!(seen, 5, "the cut-off try's bytes aren't counted twice");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        let addr = flaky_server(br#"{"a":1}"#, 3).await;
        let v: serde_json::Value = Http::new()
            .unwrap()
            .get_json(&format!("http://{addr}/v"))
            .await
            .unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn batches_can_run_on_spawned_tasks() {
        fn assert_send<T: Send>(_: &T) {}
        let http = Http::new().unwrap();
        let batch = http.download_all(Vec::new(), 4, |_| {});
        assert_send(&batch);
    }

    #[tokio::test]
    async fn sha256_is_checked_too() {
        let server = server_with("/f", ResponseTemplate::new(200).set_body_string("hello")).await;
        let dir = tempfile::tempdir().unwrap();
        let job = Download {
            url: format!("{}/f", server.uri()),
            dest: dir.path().join("f"),
            hash: Some(Hash::Sha256(
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into(),
            )),
            size: None,
        };
        Http::new().unwrap().download(&job, |_| {}).await.unwrap();
        assert!(job.hash.as_ref().unwrap().matches_file(&job.dest).await);
    }
}

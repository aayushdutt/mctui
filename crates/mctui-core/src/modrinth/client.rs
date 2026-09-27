//! The Modrinth v2 API endpoints mctui uses.

use std::collections::HashMap;

use reqwest::StatusCode;
use serde::de::DeserializeOwned;

use super::model::{Project, SearchResults, Version};
use super::{Error, Result};
use crate::net::{self, Http};

/// The production Modrinth API.
pub const API_URL: &str = "https://api.modrinth.com/v2";

/// A mod's page on modrinth.com, for opening in a browser. The id or slug
/// comes from Modrinth, so it's escaped into one path segment there.
pub fn project_page(id_or_slug: &str) -> String {
    endpoint("https://modrinth.com", &["mod", id_or_slug], &[])
}

/// A Modrinth API client. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Client {
    pub(super) http: Http,
    base_url: String,
}

impl Client {
    /// A client for the real Modrinth API.
    pub fn new(http: Http) -> Self {
        Self::with_base_url(http, API_URL)
    }

    /// A client for the API at `base_url` (a test server, say).
    pub fn with_base_url(http: Http, base_url: impl Into<String>) -> Self {
        Self {
            http,
            base_url: base_url.into(),
        }
    }

    /// Searches mods for Minecraft `game_version` with `loader` ("fabric").
    /// An empty `query` browses the most downloaded; otherwise results are by
    /// relevance. `offset` and `limit` page through the results; Modrinth
    /// sends at most 100 at a time.
    pub async fn search(
        &self,
        query: &str,
        game_version: &str,
        loader: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults> {
        let query = query.trim();
        let facets = serde_json::json!([
            [format!("categories:{loader}")],
            [format!("versions:{game_version}")],
            ["project_type:mod"],
        ])
        .to_string();
        let (offset, limit) = (offset.to_string(), limit.to_string());
        let mut params = vec![
            ("facets", facets.as_str()),
            ("offset", &offset),
            ("limit", &limit),
        ];
        if query.is_empty() {
            params.push(("index", "downloads"));
        } else {
            params.extend([("index", "relevance"), ("query", query)]);
        }
        self.get(&["search"], &params).await
    }

    /// A project by id or slug.
    pub async fn project(&self, id_or_slug: &str) -> Result<Project> {
        self.get(&["project", id_or_slug], &[]).await
    }

    /// The projects with these ids or slugs, in one request. Ones that don't
    /// exist are left out.
    pub async fn projects(&self, ids: &[&str]) -> Result<Vec<Project>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.get(&["projects"], &[("ids", &json_array(ids))]).await
    }

    /// A project's versions for any of `loaders` and any of `game_versions`;
    /// an empty list doesn't filter.
    pub async fn versions(
        &self,
        project: &str,
        loaders: &[&str],
        game_versions: &[&str],
    ) -> Result<Vec<Version>> {
        let (loaders, game_versions) = (json_array(loaders), json_array(game_versions));
        let params: Vec<_> = [("loaders", &loaders), ("game_versions", &game_versions)]
            .into_iter()
            .filter(|(_, list)| *list != "[]")
            .map(|(key, list)| (key, list.as_str()))
            .collect();
        self.get(&["project", project, "version"], &params).await
    }

    /// A version by id.
    pub async fn version(&self, id: &str) -> Result<Version> {
        self.get(&["version", id], &[]).await
    }

    /// The version a file belongs to, found by the file's SHA-1; `None` if the
    /// file isn't on Modrinth.
    pub async fn version_from_sha1(&self, sha1: &str) -> Result<Option<Version>> {
        let version = self
            .get(&["version_file", sha1], &[("algorithm", "sha1")])
            .await;
        found(version)
    }

    /// The versions files belong to, found by their SHA-1s in one request, by
    /// hash. Files that aren't on Modrinth are left out.
    pub async fn versions_from_sha1s(&self, sha1s: &[&str]) -> Result<HashMap<String, Version>> {
        if sha1s.is_empty() {
            return Ok(HashMap::new());
        }
        let url = endpoint(&self.base_url, &["version_files"], &[]);
        let body = serde_json::json!({ "hashes": sha1s, "algorithm": "sha1" });
        let failed = |source| net::Error::Request {
            url: url.clone(),
            source,
        };
        let response = self
            .http
            .client()
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(failed)?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(failed)?;
        if !status.is_success() {
            return Err(net::Error::Status { url, status }.into());
        }
        serde_json::from_slice(&bytes).map_err(|source| net::Error::Decode { url, source }.into())
    }

    async fn get<T: DeserializeOwned>(&self, path: &[&str], query: &[(&str, &str)]) -> Result<T> {
        let url = endpoint(&self.base_url, path, query);
        Ok(self.http.get_json(&url).await?)
    }
}

/// Turns a 404 into `None`.
pub(super) fn found<T>(result: Result<T>) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(Error::Net(e)) if e.status() == Some(StatusCode::NOT_FOUND) => Ok(None),
        Err(e) => Err(e),
    }
}

/// `base` with `path` segments (escaped) and `query` appended. A `base` that
/// isn't a URL is returned as is, for the request to report.
fn endpoint(base: &str, path: &[&str], query: &[(&str, &str)]) -> String {
    let Ok(mut url) = reqwest::Url::parse(base) else {
        return base.to_owned();
    };
    if let Ok(mut segments) = url.path_segments_mut() {
        segments.pop_if_empty().extend(path);
    }
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    url.into()
}

/// The API takes lists as JSON arrays: `["fabric"]`.
fn json_array(items: &[&str]) -> String {
    serde_json::Value::from(items.to_vec()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_escape_their_parts() {
        for (base, path, query, want) in [
            (
                API_URL,
                &["project", "sodium"][..],
                &[][..],
                "https://api.modrinth.com/v2/project/sodium",
            ),
            (
                "http://127.0.0.1:9/",
                &["project", "a/b?c", "version"],
                &[("loaders", r#"["fabric"]"#)],
                "http://127.0.0.1:9/project/a%2Fb%3Fc/version?loaders=%5B%22fabric%22%5D",
            ),
            (
                "http://127.0.0.1:9",
                &["version", ".."],
                &[],
                "http://127.0.0.1:9/version",
            ),
            ("not a url", &["search"], &[], "not a url"),
        ] {
            assert_eq!(endpoint(base, path, query), want, "{path:?}");
        }
    }

    #[test]
    fn project_pages_stay_on_modrinth() {
        assert_eq!(project_page("sodium"), "https://modrinth.com/mod/sodium");
        assert_eq!(
            project_page("//evil.example/x?y#z"),
            "https://modrinth.com/mod/%2F%2Fevil.example%2Fx%3Fy%23z"
        );
        for id in ["..", "@evil.example", "a b\"&|"] {
            let page = reqwest::Url::parse(&project_page(id)).unwrap();
            assert_eq!(page.host_str(), Some("modrinth.com"), "{id:?}");
        }
    }

    #[test]
    fn lists_are_json_arrays() {
        assert_eq!(json_array(&["fabric", "quilt"]), r#"["fabric","quilt"]"#);
        assert_eq!(json_array(&[]), "[]");
    }
}

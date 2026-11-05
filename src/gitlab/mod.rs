//! Thin GitLab REST API v4 client (GitLab 15.7+), see docs/adr/0004.

mod url;

pub use url::{MrRef, parse_mr_url};

use anyhow::{Context, bail};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::{Response, StatusCode};
use serde::Deserialize;

/// Everything except unreserved characters: `group/project` -> `group%2Fproject`.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

const PER_PAGE: u32 = 100;

#[derive(Debug, Clone, Deserialize)]
pub struct MergeRequest {
    pub iid: u64,
    pub title: String,
    pub state: String,
    pub source_branch: String,
    pub target_branch: String,
    pub author: User,
    /// `null` for merge requests without commits.
    pub diff_refs: Option<DiffRefs>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub username: String,
}

/// Commits the MR diff is built from; also required to post inline comments.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DiffRefs {
    /// Merge base of source and target: the "old" side of the diff.
    pub base_sha: String,
    /// Source branch head: the "new" side.
    pub head_sha: String,
    /// Target branch head when the diff was computed.
    pub start_sha: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MrDiff {
    pub old_path: String,
    pub new_path: String,
    /// Unified diff hunks (`@@ ... @@` and lines), without file headers.
    pub diff: String,
    pub new_file: bool,
    pub renamed_file: bool,
    pub deleted_file: bool,
    #[serde(default)]
    pub too_large: Option<bool>,
    #[serde(default)]
    pub collapsed: Option<bool>,
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    /// `https://host/api/v4`, without trailing slash.
    api: String,
    token: String,
}

impl Client {
    pub fn new(base_url: &str, token: String) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("rvu/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("cannot create HTTP client")?;
        Ok(Self { http, api: format!("{}/api/v4", base_url.trim_end_matches('/')), token })
    }

    async fn send(&self, path: &str, query: &[(&str, String)]) -> anyhow::Result<(String, Response)> {
        let url = format!("{}{path}", self.api);
        tracing::debug!(%url, ?query, "GET");
        let resp = self
            .http
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .query(query)
            .send()
            .await
            .with_context(|| format!("request to {url} failed"))?;
        Ok((url, resp))
    }

    /// GET with the status checked: non-2xx becomes a readable error.
    async fn get(&self, path: &str, query: &[(&str, String)]) -> anyhow::Result<Response> {
        let (url, resp) = self.send(path, query).await?;
        match resp.status() {
            s if s.is_success() => Ok(resp),
            StatusCode::UNAUTHORIZED => bail!("GitLab rejected the token (401): check it and its `api` scope"),
            StatusCode::FORBIDDEN => bail!("access denied (403) to {url}"),
            StatusCode::NOT_FOUND => bail!("not found (404): {url} — wrong URL or no access to the project"),
            s => {
                let body = resp.text().await.unwrap_or_default();
                bail!("GitLab returned {s} for {url}: {}", body.chars().take(300).collect::<String>())
            }
        }
    }

    fn mr_path(project: &str, iid: u64) -> String {
        format!("/projects/{}/merge_requests/{iid}", utf8_percent_encode(project, SEGMENT))
    }

    pub async fn merge_request(&self, project: &str, iid: u64) -> anyhow::Result<MergeRequest> {
        let resp = self.get(&Self::mr_path(project, iid), &[]).await?;
        resp.json().await.context("unexpected merge request JSON")
    }

    /// All file diffs of the MR, following pagination.
    pub async fn merge_request_diffs(&self, project: &str, iid: u64) -> anyhow::Result<Vec<MrDiff>> {
        let path = format!("{}/diffs", Self::mr_path(project, iid));
        let mut all = Vec::new();
        let mut page = 1u32;
        loop {
            let query = [("per_page", PER_PAGE.to_string()), ("page", page.to_string())];
            let resp = self.get(&path, &query).await?;
            let next = resp
                .headers()
                .get("x-next-page")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u32>().ok());
            let diffs: Vec<MrDiff> = resp.json().await.context("unexpected diffs JSON")?;
            let empty = diffs.is_empty();
            all.extend(diffs);
            match next {
                Some(n) if n > page && !empty => page = n,
                _ => break,
            }
        }
        Ok(all)
    }

    /// Raw file contents at `sha`; `None` if the file does not exist there.
    pub async fn raw_file(&self, project: &str, path: &str, sha: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let url_path = format!(
            "/projects/{}/repository/files/{}/raw",
            utf8_percent_encode(project, SEGMENT),
            utf8_percent_encode(path, SEGMENT)
        );
        let (url, resp) = self.send(&url_path, &[("ref", sha.to_owned())]).await?;
        match resp.status() {
            StatusCode::NOT_FOUND => Ok(None),
            s if s.is_success() => Ok(Some(resp.bytes().await?.to_vec())),
            s => bail!("GitLab returned {s} for {url}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn diff_json(name: &str) -> serde_json::Value {
        serde_json::json!({
            "old_path": name, "new_path": name, "a_mode": "100644", "b_mode": "100644",
            "diff": "@@ -1 +1 @@\n-a\n+b\n",
            "new_file": false, "renamed_file": false, "deleted_file": false
        })
    }

    #[tokio::test]
    async fn diffs_follow_pagination_and_encode_project() {
        let server = MockServer::start().await;
        let base = "/api/v4/projects/group%2Fsub%2Fproj/merge_requests/7/diffs";
        Mock::given(method("GET"))
            .and(path(base))
            .and(query_param("page", "1"))
            .and(header("PRIVATE-TOKEN", "tok"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-next-page", "2")
                    .set_body_json(serde_json::json!([diff_json("a.rs")])),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(base))
            .and(query_param("page", "2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-next-page", "")
                    .set_body_json(serde_json::json!([diff_json("b.rs")])),
            )
            .mount(&server)
            .await;

        let client = Client::new(&server.uri(), "tok".into()).unwrap();
        let diffs = client.merge_request_diffs("group/sub/proj", 7).await.unwrap();
        let names: Vec<_> = diffs.iter().map(|d| d.new_path.as_str()).collect();
        assert_eq!(names, ["a.rs", "b.rs"]);
    }

    #[tokio::test]
    async fn raw_file_encodes_path_and_handles_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/g%2Fp/repository/files/src%2Fmain.rs/raw"))
            .and(query_param("ref", "abc"))
            .respond_with(ResponseTemplate::new(200).set_body_string("fn main() {}\n"))
            .mount(&server)
            .await;
        let client = Client::new(&server.uri(), "t".into()).unwrap();
        let body = client.raw_file("g/p", "src/main.rs", "abc").await.unwrap();
        assert_eq!(body.as_deref(), Some(&b"fn main() {}\n"[..]));
        assert_eq!(client.raw_file("g/p", "missing.rs", "abc").await.unwrap(), None);
    }

    #[tokio::test]
    async fn auth_error_is_explained() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let client = Client::new(&server.uri(), "bad".into()).unwrap();
        let err = client.merge_request("g/p", 1).await.unwrap_err().to_string();
        assert!(err.contains("token"), "{err}");
    }
}

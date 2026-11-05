//! Parsing merge request URLs.

use anyhow::{Context, anyhow, bail};

/// `https://gitlab.example.com/group/sub/project/-/merge_requests/42`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MrRef {
    /// `https://gitlab.example.com` (with port if any).
    pub base_url: String,
    /// `gitlab.example.com` or `gitlab.example.com:8443` — key for `[[gitlab]]` in config.
    pub host: String,
    /// `group/sub/project`.
    pub project: String,
    pub iid: u64,
}

pub fn parse_mr_url(s: &str) -> anyhow::Result<MrRef> {
    let url = url::Url::parse(s).with_context(|| format!("invalid URL {s:?}"))?;
    let host_name = url.host_str().ok_or_else(|| anyhow!("URL {s:?} has no host"))?;
    let host = match url.port() {
        Some(p) => format!("{host_name}:{p}"),
        None => host_name.to_owned(),
    };
    let (project, rest) = url
        .path()
        .split_once("/-/merge_requests/")
        .ok_or_else(|| anyhow!("{s:?} is not a merge request URL (expected .../-/merge_requests/<id>)"))?;
    let project = project.trim_matches('/');
    if project.is_empty() {
        bail!("no project path in {s:?}");
    }
    let iid_str: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let iid = iid_str.parse().with_context(|| format!("no merge request number in {s:?}"))?;
    Ok(MrRef {
        base_url: format!("{}://{host}", url.scheme()),
        host,
        project: project.to_owned(),
        iid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_urls() {
        let r = parse_mr_url("https://gitlab.example.com/group/sub/proj/-/merge_requests/42").unwrap();
        assert_eq!(
            r,
            MrRef {
                base_url: "https://gitlab.example.com".into(),
                host: "gitlab.example.com".into(),
                project: "group/sub/proj".into(),
                iid: 42
            }
        );
        // Tabs, anchors, query strings and ports are fine.
        let r = parse_mr_url("http://git.local:8080/a/b/-/merge_requests/7/diffs?commit_id=x#note_1").unwrap();
        assert_eq!((r.host.as_str(), r.base_url.as_str()), ("git.local:8080", "http://git.local:8080"));
        assert_eq!((r.project.as_str(), r.iid), ("a/b", 7));
    }

    #[test]
    fn rejects_non_mr_urls() {
        assert!(parse_mr_url("https://gitlab.example.com/group/proj").is_err());
        assert!(parse_mr_url("https://gitlab.example.com/group/proj/-/merge_requests/").is_err());
        assert!(parse_mr_url("not a url").is_err());
    }
}

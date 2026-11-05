//! GitLab merge request as a source of changes (everything via the API).

use std::path::PathBuf;

use anyhow::{Context, anyhow};

use crate::config::Config;
use crate::gitlab::{Client, DiffRefs, MrDiff, MrRef, parse_mr_url};
use crate::model::{Changeset, FileDiff, FileStatus, Side, Text};
use crate::source::unidiff::parse_hunks;

/// What we show about the MR in the UI.
#[derive(Debug, Clone)]
pub struct MrInfo {
    pub iid: u64,
    pub title: String,
    pub state: String,
    pub source_branch: String,
    pub target_branch: String,
    pub author: String,
    pub refs: DiffRefs,
}

pub struct MergeRequestSource {
    client: Client,
    mr: MrRef,
}

/// One file version to fetch for highlighting and Page mode.
#[derive(Debug, Clone)]
pub struct ContentRequest {
    pub path: PathBuf,
    pub side: Side,
    pub sha: String,
}

impl MergeRequestSource {
    pub fn new(config: &Config, url: &str) -> anyhow::Result<Self> {
        let mr = parse_mr_url(url)?;
        let instance = config.gitlab_for(&mr.host).ok_or_else(|| {
            anyhow!(
                "no GitLab instance for host {:?} in config: add\n\n[[gitlab]]\nhost = {:?}\ntoken_env = \"RVU_GITLAB_TOKEN\"\n",
                mr.host,
                mr.host
            )
        })?;
        let token = instance.token()?;
        Ok(Self { client: Client::new(&mr.base_url, token)?, mr })
    }

    pub async fn load(&self) -> anyhow::Result<(MrInfo, Changeset)> {
        let (mr, diffs) = tokio::try_join!(
            self.client.merge_request(&self.mr.project, self.mr.iid),
            self.client.merge_request_diffs(&self.mr.project, self.mr.iid),
        )?;
        let refs = mr.diff_refs.ok_or_else(|| anyhow!("merge request !{} has no commits yet", mr.iid))?;
        let files = diffs.into_iter().map(file_diff).collect::<anyhow::Result<_>>()?;
        let info = MrInfo {
            iid: mr.iid,
            title: mr.title,
            state: mr.state,
            source_branch: mr.source_branch,
            target_branch: mr.target_branch,
            author: mr.author.username,
            refs,
        };
        Ok((info, Changeset { files }))
    }

    pub async fn content(&self, req: &ContentRequest) -> anyhow::Result<Option<Text>> {
        let path = req.path.to_string_lossy();
        let bytes = self.client.raw_file(&self.mr.project, &path, &req.sha).await?;
        Ok(bytes.map(|b| Text::from(String::from_utf8_lossy(&b).into_owned())))
    }
}

/// File versions to fetch: old side at the merge base, new side at the head.
pub fn content_requests(cs: &Changeset, refs: &DiffRefs) -> Vec<ContentRequest> {
    let mut out = Vec::new();
    for f in cs.files.iter().filter(|f| !f.binary && !f.too_large) {
        if let Some(p) = &f.old_path {
            out.push(ContentRequest { path: p.clone(), side: Side::Old, sha: refs.base_sha.clone() });
        }
        if let Some(p) = &f.new_path {
            out.push(ContentRequest { path: p.clone(), side: Side::New, sha: refs.head_sha.clone() });
        }
    }
    out
}

fn file_diff(d: MrDiff) -> anyhow::Result<FileDiff> {
    let status = if d.new_file {
        FileStatus::Added
    } else if d.deleted_file {
        FileStatus::Deleted
    } else if d.renamed_file {
        FileStatus::Renamed
    } else {
        FileStatus::Modified
    };
    let hunks = parse_hunks(&d.diff).with_context(|| format!("cannot parse diff of {}", d.new_path))?;
    let binary = hunks.is_empty() && d.diff.starts_with("Binary files");
    let too_large = d.too_large == Some(true) || (d.collapsed == Some(true) && hunks.is_empty());
    Ok(FileDiff {
        old_path: (!d.new_file).then(|| PathBuf::from(&d.old_path)),
        new_path: (!d.deleted_file).then(|| PathBuf::from(&d.new_path)),
        status,
        binary,
        too_large,
        hunks,
        old_content: None,
        new_content: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(old: &str, new: &str, text: &str, flags: (bool, bool, bool)) -> MrDiff {
        MrDiff {
            old_path: old.into(),
            new_path: new.into(),
            diff: text.into(),
            new_file: flags.0,
            renamed_file: flags.1,
            deleted_file: flags.2,
            too_large: None,
            collapsed: None,
        }
    }

    #[test]
    fn converts_statuses_and_paths() {
        let f = file_diff(diff("a", "a", "@@ -0,0 +1 @@\n+x\n", (true, false, false))).unwrap();
        assert_eq!((f.status, f.old_path, f.new_path.is_some()), (FileStatus::Added, None, true));

        let f = file_diff(diff("a", "a", "@@ -1 +0,0 @@\n-x\n", (false, false, true))).unwrap();
        assert_eq!((f.status, f.new_path), (FileStatus::Deleted, None));

        let f = file_diff(diff("a", "b", "", (false, true, false))).unwrap();
        assert_eq!(f.status, FileStatus::Renamed);
        assert_eq!(f.path(), std::path::Path::new("b"));

        let f = file_diff(diff("x.png", "x.png", "Binary files a/x.png and b/x.png differ\n", (false, false, false))).unwrap();
        assert!(f.binary);

        let mut d = diff("big", "big", "", (false, false, false));
        d.too_large = Some(true);
        assert!(file_diff(d).unwrap().too_large);
    }

    #[test]
    fn requests_old_at_base_and_new_at_head() {
        let refs = DiffRefs { base_sha: "base".into(), head_sha: "head".into(), start_sha: "start".into() };
        let cs = Changeset {
            files: vec![
                file_diff(diff("m", "m", "@@ -1 +1 @@\n-a\n+b\n", (false, false, false))).unwrap(),
                file_diff(diff("n", "n", "@@ -0,0 +1 @@\n+a\n", (true, false, false))).unwrap(),
            ],
        };
        let reqs: Vec<_> = content_requests(&cs, &refs)
            .into_iter()
            .map(|r| (r.path.to_string_lossy().into_owned(), r.side, r.sha))
            .collect();
        assert_eq!(
            reqs,
            [
                ("m".into(), Side::Old, "base".into()),
                ("m".into(), Side::New, "head".into()),
                ("n".into(), Side::New, "head".into()),
            ]
        );
    }
}

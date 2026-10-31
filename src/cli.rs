use std::path::PathBuf;

use anyhow::bail;
use clap::Parser;

/// Terminal code review client.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Path to config file (default: $XDG_CONFIG_HOME/rvu/config.toml).
    #[arg(long, env = "RVU_CONFIG")]
    pub config: Option<PathBuf>,

    /// What to review: GitLab merge request URL. Omit to review `git diff`.
    pub target: Option<String>,
}

/// Source of the changes being reviewed.
#[derive(Debug, Clone)]
pub enum Target {
    /// Working tree vs index, like `git diff`.
    WorkingTree,
    /// GitLab merge request, by full URL.
    GitLabMr { url: String },
}

impl Cli {
    pub fn target(&self) -> anyhow::Result<Target> {
        match self.target.as_deref() {
            None => Ok(Target::WorkingTree),
            Some(s) if s.starts_with("https://") || s.starts_with("http://") => {
                Ok(Target::GitLabMr { url: s.to_owned() })
            }
            Some(s) => bail!("unsupported target {s:?}: expected a merge request URL"),
        }
    }
}

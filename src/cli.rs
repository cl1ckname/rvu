use std::path::PathBuf;

use anyhow::bail;
use clap::Parser;

use crate::source::local::DiffSpec;

/// Terminal code review client.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Path to config file (default: $XDG_CONFIG_HOME/rvu/config.toml).
    #[arg(long, env = "RVU_CONFIG")]
    pub config: Option<PathBuf>,

    /// Compare the index instead of the working tree, like `git diff --staged`.
    #[arg(long, visible_alias = "cached")]
    pub staged: bool,

    /// What to review: a revision as in `git diff` (`main`, `main..HEAD`,
    /// `main...`, `abc123^!`) or a GitLab merge request URL.
    /// Omit to review `git diff`.
    pub target: Option<String>,
}

/// Source of the changes being reviewed.
#[derive(Debug, Clone)]
pub enum Target {
    /// Local repository, like `git diff [<rev>]`.
    Local(DiffSpec),
    /// GitLab merge request, by full URL.
    GitLabMr { url: String },
}

impl Cli {
    pub fn target(&self) -> anyhow::Result<Target> {
        match self.target.as_deref() {
            Some(s) if s.starts_with("https://") || s.starts_with("http://") => {
                if self.staged {
                    bail!("--staged does not apply to merge requests");
                }
                Ok(Target::GitLabMr { url: s.to_owned() })
            }
            arg => Ok(Target::Local(DiffSpec::parse(arg, self.staged)?)),
        }
    }
}

//! Configuration file, see `docs/config.md`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use serde::Deserialize;

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub ui: UiConfig,
    pub page: PageConfig,
    pub comments: CommentsConfig,
    pub gitlab: Vec<GitLabInstance>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub default_mode: ViewMode,
    pub theme: String,
    /// Show the file tree on startup (toggle with `Ctrl-b`).
    pub show_tree: bool,
    pub tree_width: u16,
    pub context_lines: u32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            default_mode: ViewMode::Scroll,
            theme: "base16-ocean.dark".to_owned(),
            show_tree: true,
            tree_width: 30,
            context_lines: 3,
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PageConfig {
    /// Hide unchanged regions in Page mode on startup (toggle with `zi`).
    pub fold_unchanged: bool,
    /// Unchanged lines kept visible above and below each change when folding.
    pub fold_context: usize,
}

impl Default for PageConfig {
    fn default() -> Self {
        Self { fold_unchanged: false, fold_context: 5 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    /// Continuous diff of all files, like `git diff`.
    Scroll,
    /// One whole file with highlighted hunks.
    Page,
}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CommentsConfig {
    /// File extension -> comment syntax override.
    pub syntax: HashMap<String, CommentSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum CommentSyntax {
    /// `"//"` -> `// text`
    Line(String),
    /// `["/*", "*/"]` -> `/* text */`
    Block([String; 2]),
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitLabInstance {
    pub host: String,
    pub token: Option<String>,
    pub token_env: Option<String>,
    pub token_command: Option<String>,
}

impl GitLabInstance {
    /// Resolves the access token from whichever source is configured.
    #[allow(dead_code)] // used from M4 (GitLab)
    pub fn token(&self) -> anyhow::Result<String> {
        match (&self.token, &self.token_env, &self.token_command) {
            (Some(token), None, None) => Ok(token.clone()),
            (None, Some(var), None) => {
                std::env::var(var).with_context(|| format!("env variable {var} is not set"))
            }
            (None, None, Some(cmd)) => {
                let out = Command::new("sh")
                    .args(["-c", cmd])
                    .output()
                    .with_context(|| format!("failed to run token_command {cmd:?}"))?;
                if !out.status.success() {
                    bail!("token_command {cmd:?} exited with {}", out.status);
                }
                Ok(String::from_utf8(out.stdout)
                    .context("token_command output is not UTF-8")?
                    .trim()
                    .to_owned())
            }
            _ => unreachable!("validated in Config::validate"),
        }
    }
}

impl Config {
    /// Default location: `$XDG_CONFIG_HOME/rvu/config.toml`.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("rvu").join("config.toml"))
    }

    /// Loads config from `explicit` path (must exist) or from the default
    /// location (may be absent — defaults are used then).
    pub fn load(explicit: Option<&Path>) -> anyhow::Result<Self> {
        let (path, required) = match explicit {
            Some(p) => (p.to_path_buf(), true),
            None => match Self::default_path() {
                Some(p) => (p, false),
                None => return Ok(Self::default()),
            },
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => {
                return Ok(Self::default());
            }
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        Self::parse(&text).with_context(|| format!("invalid config {}", path.display()))
    }

    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let config: Self = toml::from_str(text)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> anyhow::Result<()> {
        for gl in &self.gitlab {
            let sources = [gl.token.is_some(), gl.token_env.is_some(), gl.token_command.is_some()];
            if sources.iter().filter(|s| **s).count() != 1 {
                bail!(
                    "gitlab host {:?}: set exactly one of token, token_env, token_command",
                    gl.host
                );
            }
        }
        Ok(())
    }

    /// Finds the GitLab instance configured for `host`.
    #[allow(dead_code)] // used from M4 (GitLab)
    pub fn gitlab_for(&self, host: &str) -> Option<&GitLabInstance> {
        self.gitlab.iter().find(|g| g.host == host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `assets/config.toml` documents the defaults: keep it in sync.
    #[test]
    fn default_config_file_matches_defaults() {
        let file = Config::parse(include_str!("../assets/config.toml")).unwrap();
        assert_eq!(file, Config::default());
    }

    #[test]
    fn empty_config_uses_defaults() {
        let c = Config::parse("").unwrap();
        assert_eq!(c.ui.default_mode, ViewMode::Scroll);
        assert_eq!(c.ui.tree_width, 30);
        assert!(c.gitlab.is_empty());
    }

    #[test]
    fn full_example_parses() {
        let c = Config::parse(
            r#"
            [ui]
            default_mode = "page"
            theme = "InspiredGitHub"
            tree_width = 40
            context_lines = 5

            [page]
            fold_unchanged = true
            fold_context = 10

            [comments.syntax]
            rs = "//"
            vue = ["<!--", "-->"]

            [[gitlab]]
            host = "gitlab.example.com"
            token_env = "RVU_GITLAB_TOKEN"
            "#,
        )
        .unwrap();
        assert_eq!(c.ui.default_mode, ViewMode::Page);
        assert_eq!(c.page, PageConfig { fold_unchanged: true, fold_context: 10 });
        assert_eq!(c.comments.syntax["rs"], CommentSyntax::Line("//".into()));
        assert_eq!(
            c.comments.syntax["vue"],
            CommentSyntax::Block(["<!--".into(), "-->".into()])
        );
        assert!(c.gitlab_for("gitlab.example.com").is_some());
    }

    #[test]
    fn unknown_key_is_error() {
        assert!(Config::parse("[ui]\ntheem = \"x\"").is_err());
    }

    #[test]
    fn gitlab_requires_exactly_one_token_source() {
        assert!(Config::parse("[[gitlab]]\nhost = \"h\"").is_err());
        assert!(Config::parse("[[gitlab]]\nhost = \"h\"\ntoken = \"a\"\ntoken_env = \"B\"").is_err());
    }

    #[test]
    fn token_command_output_is_trimmed() {
        let c = Config::parse("[[gitlab]]\nhost = \"h\"\ntoken_command = \"echo '  glpat-x  '\"").unwrap();
        assert_eq!(c.gitlab[0].token().unwrap(), "glpat-x");
    }
}

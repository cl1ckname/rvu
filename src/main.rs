mod app;
mod cli;
mod comment;
mod config;
mod gitlab;
mod highlight;
mod input;
mod keymap;
mod logging;
mod model;
mod source;
mod tree;
mod ui;
mod view;

use clap::Parser;

use std::sync::Arc;

use crate::cli::Target;
use crate::source::Source;
use crate::source::gitlab::MergeRequestSource;
use crate::source::local::LocalRepo;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let target = cli.target()?;
    let config = config::Config::load(cli.config.as_deref())?;
    let _log_guard = logging::init()?;
    tracing::info!(?target, "starting rvu");

    let highlighter = highlight::Highlighter::new(&config.ui.theme)?;

    // Load before entering the TUI so that errors are printed normally.
    let (source, changeset) = match &target {
        Target::Local(spec) => {
            let repo = LocalRepo::discover(&std::env::current_dir()?, spec.clone())?;
            let cs = repo.diff(config.ui.context_lines)?;
            (Source::Local(repo), cs)
        }
        Target::GitLabMr { url } => {
            let mr = MergeRequestSource::new(&config, url)?;
            eprintln!("Loading {url} ...");
            let (info, cs) = mr.load().await?;
            (Source::MergeRequest { source: Arc::new(mr), info }, cs)
        }
    };

    // ratatui::init installs a panic hook that restores the terminal.
    let terminal = ratatui::init();
    let result = app::App::new(config, source, changeset, highlighter).run(terminal).await;
    ratatui::restore();
    result
}

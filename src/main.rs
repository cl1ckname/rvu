mod app;
mod cli;
mod comment;
mod config;
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

use crate::cli::Target;
use crate::model::Changeset;
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
    let (repo, changeset) = match &target {
        Target::WorkingTree => {
            let repo = LocalRepo::discover(&std::env::current_dir()?)?;
            let cs = repo.working_tree_diff(config.ui.context_lines)?;
            (Some(repo), cs)
        }
        Target::GitLabMr { .. } => (None, Changeset::default()),
    };

    // ratatui::init installs a panic hook that restores the terminal.
    let terminal = ratatui::init();
    let result = app::App::new(config, target, repo, changeset, highlighter).run(terminal).await;
    ratatui::restore();
    result
}

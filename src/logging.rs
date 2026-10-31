use anyhow::Context;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

/// Logs go to `$XDG_STATE_HOME/rvu/rvu.log`: stdout belongs to the TUI.
/// Level is controlled by `RVU_LOG` (default `info`).
pub fn init() -> anyhow::Result<WorkerGuard> {
    let dir = dirs::state_dir()
        .context("cannot determine state directory")?
        .join("rvu");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("cannot create log directory {}", dir.display()))?;

    let (writer, guard) = tracing_appender::non_blocking(tracing_appender::rolling::never(&dir, "rvu.log"));
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(EnvFilter::try_from_env("RVU_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .init();
    Ok(guard)
}

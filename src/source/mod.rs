//! Where changes come from.

pub mod gitlab;
pub mod local;
pub mod unidiff;

use std::sync::Arc;

/// The changes under review.
pub enum Source {
    /// `git diff` in a local repository.
    Local(local::LocalRepo),
    /// GitLab merge request (everything via the API).
    MergeRequest { source: Arc<gitlab::MergeRequestSource>, info: gitlab::MrInfo },
}

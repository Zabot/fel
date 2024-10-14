use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use git2::Oid;
use git2::PushOptions;
use git2::Remote;
use git2::RemoteCallbacks;
use parking_lot::Mutex;
use tokio::sync::oneshot;
use tokio::sync::Notify;

#[derive(Clone)]
struct Refspec {
    commit: Oid,
    branch: String,
    force: bool,
}

impl ToString for Refspec {
    fn to_string(&self) -> String {
        let refname = self.refname();
        format!(
            "{}{}:{}",
            if self.force { "+" } else { "" },
            self.commit,
            refname,
        )
    }
}

impl Refspec {
    fn new(commit: Oid, branch: String, force: bool) -> Self {
        let branch = branch.strip_prefix('/').unwrap_or(&branch);
        Self {
            commit,
            branch: branch.to_string(),
            force,
        }
    }

    fn refname(&self) -> String {
        PathBuf::from("refs/heads")
            .join(&self.branch)
            .display()
            .to_string()
    }
}

struct PendingPush {
    refspec: Refspec,
    info: oneshot::Sender<Result<(), PushError>>,
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum PushError {
    #[error("push rejected by remote: {0}")]
    Rejected(String),

    #[error("cancelled by client")]
    Cancelled,
}

#[derive(Default)]
pub struct BatchedPusher {
    pending: Mutex<Vec<PendingPush>>,
    new_task: Notify,
}

impl BatchedPusher {
    /// Push `commit` to the new head of `branch`. `force` overwrites existing references
    #[tracing::instrument(skip(self))]
    pub async fn push(&self, commit: Oid, branch: String, force: bool) -> Result<(), PushError> {
        let (tx, rx) = oneshot::channel();

        tracing::debug!("waiting for pending lock");
        self.pending.lock().push(PendingPush {
            refspec: Refspec::new(commit, branch, force),
            info: tx,
        });

        tracing::debug!("queued push");
        self.new_task.notify_waiters();
        rx.await.unwrap_or(Err(PushError::Cancelled))
    }

    /// Wait until `count` branches are ready to be pushed, and then push them all
    /// together to `remote`. Push failures are reported to the individual `push`
    /// calls.
    #[tracing::instrument(skip(self, remote), fields(remote=remote.name()))]
    pub async fn wait_for(&self, count: usize, remote: &mut Remote<'_>) -> Result<()> {
        tracing::debug!("waiting for pending pushes");
        let pending = loop {
            {
                let mut pending_guard = self.pending.lock();

                tracing::trace!(count = pending_guard.len(), "waiting...");
                if pending_guard.len() >= count {
                    let old: Vec<PendingPush> = std::mem::take(pending_guard.as_mut());
                    break old;
                }
            }

            self.new_task.notified().await;
        };

        tracing::debug!("beginning push");
        let mut refspecs = Vec::with_capacity(pending.len());
        let mut info = HashMap::with_capacity(pending.len());
        for push in pending.into_iter() {
            refspecs.push(push.refspec.to_string());
            info.insert(push.refspec.refname(), push.info);
        }

        let mut callbacks = RemoteCallbacks::default();
        callbacks
            .sideband_progress(|message| {
                tracing::trace!(message = ?std::str::from_utf8(message), "sideband progress");
                true
            })
            .update_tips(|branch, old, new| {
                tracing::trace!(branch, ?old, ?new, "updated branch");
                true
            })
            .pack_progress(|stage, b, c| {
                tracing::trace!(?stage, b, c, "pack progress");
            })
            .push_transfer_progress(|a, b, c| {
                tracing::trace!(a, b, c, "transfer progress");
            })
            .push_negotiation(|updates| {
                let updates: Vec<_> = updates
                    .iter()
                    .map(|update| (update.src_refname(), update.dst_refname()))
                    .collect();
                tracing::trace!(?updates, "negotiation");
                Ok(())
            })
            .push_update_reference(|branch, status| {
                tracing::trace!(branch, ?status, "update reference");

                let Some(sender) = info.remove(branch) else {
                    // Got update for branch we didn't push
                    tracing::warn!(branch, "unsolicited update to branch");
                    return Ok(());
                };

                let result = status
                    .map(|error| Err(PushError::Rejected(error.to_string())))
                    .unwrap_or(Ok(()));
                sender.send(result).ok();

                Ok(())
            });

        tracing::debug!(?refspecs, "pushing commits");
        tokio::task::block_in_place(|| {
            remote.push(
                &refspecs,
                Some(PushOptions::default().remote_callbacks(callbacks)),
            )
        })
        .context("failed to push")
    }
}

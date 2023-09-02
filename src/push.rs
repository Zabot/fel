use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use git2::Oid;
use git2::PushOptions;
use git2::Remote;
use git2::RemoteCallbacks;
use parking_lot::Mutex;
use tokio::sync::{mpsc, watch};

type PushResult = Result<String, PushError>;

#[derive(thiserror::Error, Debug, Clone)]
pub enum PushError {
    #[error("push rejected by remote: {0}")]
    Rejected(String),
}

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

pub struct Pusher {
    targets: Mutex<HashMap<Oid, watch::Sender<Option<PushResult>>>>,
    refspecs_tx: mpsc::Sender<Refspec>,
    refspecs: tokio::sync::Mutex<mpsc::Receiver<Refspec>>,
}

impl Pusher {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(16);
        Self {
            targets: Mutex::new(HashMap::new()),
            refspecs_tx: tx,
            refspecs: tokio::sync::Mutex::new(rx),
        }
    }

    pub async fn push(&self, commit: Oid, branch: String, force: bool) -> PushResult {
        let refspec = Refspec::new(commit, branch, force);

        self.targets.lock().entry(commit).or_insert_with(|| {
            let (tx, _) = watch::channel(None);
            tx
        });

        self.refspecs_tx.send(refspec).await.ok();
        self.wait(commit).await
    }

    pub async fn wait(&self, commit: Oid) -> PushResult {
        let mut rx = self
            .targets
            .lock()
            .entry(commit)
            .or_insert_with(|| {
                let (tx, _) = watch::channel(None);
                tx
            })
            .subscribe();

        let branch = rx
            .wait_for(|branch| branch.is_some())
            .await
            .expect("channel was closed")
            .clone();

        branch.expect("branch was just asserted not none")
    }

    pub async fn send(&self, batch: usize, remote: &mut Remote<'_>) -> Result<()> {
        let mut branches = HashMap::new();
        let refspecs = {
            tracing::debug!("waiting for refspecs");
            let mut lock_guard = self.refspecs.lock().await;

            let mut refspecs = Vec::with_capacity(batch);
            for _ in 0..batch {
                let Some(refspec) = lock_guard.recv().await else {
                    break
                };
                let refname = refspec.refname();
                refspecs.push(refspec.to_string());
                branches.insert(refname, refspec);
            }

            refspecs
        };

        let mut callbacks = RemoteCallbacks::default();
        callbacks
            .sideband_progress(|message| {
                tracing::trace!(message = ?std::str::from_utf8(&message), "sideband progress");
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

                let Some(refspec) = branches.get(branch) else {
                    // Got update for branch we didn't request
                    tracing::warn!(branch, "unsolicited update to branch");
                    return Ok(());
                };

                let targets = self.targets.lock();
                let Some(sender) = targets.get(&refspec.commit) else {
                    // Got update for branch we didn't push
                    tracing::warn!(branch, "unsolicited update to branch");
                    return Ok(());
                };

                let result = status
                    .map(|error| Err(PushError::Rejected(error.to_string())))
                    .unwrap_or(Ok(refspec.branch.clone()));
                sender.send_replace(Some(result));

                Ok(())
            });

        tracing::debug!(?refspecs, "pushing commits");
        tokio::task::block_in_place(|| {
            remote.push(
                &refspecs,
                Some(PushOptions::default().remote_callbacks(callbacks)),
            )
        })
        .context("failed to push")?;
        tracing::debug!("push finished");

        Ok(())
    }
}

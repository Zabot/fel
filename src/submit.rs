use anyhow::{Context, Result};
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use git2::{Remote, Repository};
use octocrab::Octocrab;

use crate::auth;
use crate::gh::GHRepo;
use crate::push::Pusher;
use crate::stack::Stack;
use crate::update::CommitUpdater;

use std::sync::Arc;

pub async fn submit(
    stack: &Stack,
    remote: &mut Remote<'_>,
    gh_repo: &GHRepo,
    octocrab: Arc<Octocrab>,
    repo: &Repository,
) -> Result<()> {
    tracing::debug!(remote = remote.name(), "connecting to remote");
    let mut conn = remote
        .connect_auth(git2::Direction::Push, Some(auth::callbacks()), None)
        .context("failed to connect to repo")?;
    tracing::debug!(connected = conn.connected(), "remote connected");

    let pusher = Arc::new(Pusher::new());

    let updater = CommitUpdater::new(
        octocrab.clone(),
        stack.name(),
        "master",
        &gh_repo,
        pusher.clone(),
    );

    let futures: Result<FuturesUnordered<_>> = stack
        .iter()
        .enumerate()
        .map(|(i, commit)| Ok(updater.update(i, repo, commit)))
        .collect();
    let futures = futures.context("failed to generate futures")?;
    let branches = futures.len();
    let mut futures = futures.collect::<Vec<_>>();

    let results = loop {
        tokio::select! {
            // TODO push gets called twice because its in a loop
            push = pusher.send(branches, conn.remote()) => push.context("failed to push")?,
            r = &mut futures => break r,
        }
    };

    for r in results {
        r.context("failed to update diff")?;
    }

    Ok(())
}

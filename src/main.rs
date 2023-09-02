use anyhow::{Context, Result};
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use git2::Repository;
use std::borrow::Borrow;
use std::sync::Arc;

mod auth;
mod config;
mod gh;
mod metadata;
mod push;
mod stack;
mod update;

use config::Config;
use push::Pusher;
use stack::Stack;
use update::CommitUpdater;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load().context("failed to load config")?;
    tracing_subscriber::fmt::init();

    // Make sure that notes.rewriteRef contains the namespace for fel notes so
    // they are copied along with commits during a rebase or ammend
    {
        let config = git2::Config::open_default().context("failed to open config")?;
        let rewrite_ref = config
            .entries(Some("notes.rewriteref"))
            .context("failed to get notes.rewriteRef")?;

        let mut found = false;
        rewrite_ref.for_each(|entry| {
            if entry.value() == Some("refs/notes/fel") {
                found = true;
            }
        })?;
        anyhow::ensure!(
            found,
            "notes.rewriteRef must include 'refs/notes/fel' for fel to work properly"
        );
    }

    let repo = Arc::new(Repository::discover("test").context("failed to open repo")?);

    // Push every commit
    let octocrab = Arc::new(
        octocrab::OctocrabBuilder::default()
            .personal_token(config.token.clone())
            .build()?,
    );

    let mut remote = repo
        .find_remote(&config.default_remote)
        .context("failed to get remote")?;

    let gh_repo = gh::get_repo(&remote).context("failed to get repo")?;

    tracing::debug!(remote = remote.name(), "connecting to remote");
    let mut conn = remote
        .connect_auth(git2::Direction::Push, Some(auth::callbacks()), None)
        .context("failed to connect to repo")?;
    tracing::debug!(connected = conn.connected(), "remote connected");

    let stack = Stack::new(&repo, &config.default_upstream).context("failed to get stack")?;

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
        .map(|(i, commit)| Ok(updater.update(i, repo.borrow(), commit)))
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

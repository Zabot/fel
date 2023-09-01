use anyhow::{Context, Result};
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use git2::BranchType;
use git2::Config;
use git2::Repository;
use git2::Sort;
use std::sync::Arc;

mod auth;
mod gh;
mod metadata;
mod push;
use push::Pusher;

use crate::metadata::Metadata;

#[tokio::main]
async fn main() -> Result<()> {
    // TODO Move these to a config file
    let gh_pat = std::env::var("GH_PAT").context("GH_PAT undefined")?;
    let default_remote = "origin";
    let default_branch = "origin/master";

    tracing_subscriber::fmt::init();

    // Make sure that notes.rewriteRef contains the namespace for fel notes so
    // they are copied along with commits during a rebase or ammend
    let config = Config::open_default().context("failed to open config")?;
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

    // Find the local HEAD
    let repo = Arc::new(Repository::discover("test").context("failed to open repo")?);
    let head = repo.head().context("failed to get head")?;
    let head_commit = head.peel_to_commit().context("failed to get head commit")?;
    let branch_name = head.shorthand().context("invalid shorthand")?;
    tracing::debug!(branch_name, ?head_commit, "found HEAD");

    // Find the remote HEAD
    let default = repo
        .find_branch(default_branch, BranchType::Remote)
        .context("failed to find default branch")?;
    let default_commit = default
        .get()
        .peel_to_commit()
        .context("failed to get default commit")?;
    tracing::debug!(?default_commit, "found default HEAD");

    // Calculate the first common ancestor
    let merge_base = repo
        .merge_base(default_commit.id(), head_commit.id())
        .context("failed to locate merge base")?;
    tracing::debug!(?merge_base, "found merge base");

    // Create an iterator over the stack
    let mut walk = repo.revwalk().context("failed to create revwalk")?;
    walk.push(head_commit.id())
        .context("failed to add commit to revwalk")?;
    walk.hide(merge_base).context("failed to hide revwalk")?;
    walk.set_sorting(Sort::REVERSE)
        .context("failed to set sorting")?;

    // Push every commit
    let octocrab = octocrab::OctocrabBuilder::default()
        .personal_token(gh_pat.clone())
        .build()?;

    let mut remote = repo
        .find_remote(default_remote)
        .context("failed to get remote")?;

    let gh_repo = gh::get_repo(&remote).context("failed to get repo")?;

    tracing::debug!(remote = remote.name(), "connecting to remote");
    let mut conn = remote
        .connect_auth(git2::Direction::Push, Some(auth::callbacks()), None)
        .context("failed to connect to repo")?;
    tracing::debug!(connected = conn.connected(), "remote connected");

    let pusher = Pusher::new();

    let futures: FuturesUnordered<_> = walk
        .enumerate()
        .map(|(i, oid)| {
            let repo = &repo;
            let octocrab = &octocrab;
            let pusher = &pusher;
            let gh_repo = &gh_repo;
            async move {
                let commit = repo.find_commit(oid?)?;
                anyhow::ensure!(
                    commit.parent_count() == 1,
                    "fel stacks cannot contain merge commits"
                );

                let metadata = Metadata::new(&repo, commit.id())?;
                let (branch, force) = match &metadata.branch {
                    Some(branch) => (branch.clone(), true),
                    None => (format!("fel/{}/{}", branch_name, i), false),
                };
                let branch = pusher
                    .push(commit.id(), branch, force)
                    .await
                    .map_err(|error| anyhow::anyhow!("failed to push branch: {}", error))?;

                // The parent branch is either the default branch, or the pushed branch of the
                // parent commit
                let base = if i == 0 {
                    String::from("master")
                } else {
                    pusher.wait(commit.parent_id(0)?).await.map_err(|error| {
                        anyhow::anyhow!("failed to get parent branch: {}", error)
                    })?
                };

                let pr = match metadata.pr {
                    None => {
                        tracing::debug!(
                            owner = gh_repo.owner,
                            repo = gh_repo.repo,
                            branch,
                            base,
                            "creating PR"
                        );
                        octocrab
                            .pulls(&gh_repo.owner, &gh_repo.repo)
                            .create(
                                commit.summary().context("commit header not valid UTF-8")?,
                                &branch,
                                &base,
                            )
                            .body(commit.body().unwrap_or(""))
                            .send()
                            .await
                            .context("failed to create pr")?
                    }
                    Some(pr) => {
                        tracing::debug!(
                            pr,
                            owner = gh_repo.owner,
                            repo = gh_repo.repo,
                            base,
                            "amending PR"
                        );
                        octocrab
                            .pulls(&gh_repo.owner, &gh_repo.repo)
                            .update(pr)
                            .base(base)
                            .send()
                            .await
                            .context("failed to update pr")?
                    }
                };

                let metadata = Metadata {
                    pr: Some(pr.number),
                    branch: Some(branch),
                };
                tracing::debug!(?metadata, ?commit, "updating commit metadata");
                metadata
                    .write(repo, &commit)
                    .context("failed to write commit metadata")?;
                Ok::<_, anyhow::Error>(())
            }
        })
        .collect();
    let branches = futures.len();
    let mut futures = futures.collect::<Vec<_>>();

    let results = loop {
        tokio::select! {
            push = pusher.send(branches, conn.remote()) => push.context("failed to push")?,
            r = &mut futures => break r,
        }
    };

    for r in results {
        r.context("failed to update diff")?;
    }

    Ok(())
}

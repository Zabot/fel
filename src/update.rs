use anyhow::{Context, Result};
use git2::{Oid, Repository};
use octocrab::Octocrab;

use crate::{gh::GHRepo, metadata::Metadata, push::Pusher};

pub async fn update_commit(
    index: usize,
    branch_name: &str,
    repo: &Repository,
    octocrab: &Octocrab,
    pusher: &Pusher,
    gh_repo: &GHRepo,
    oid: Oid,
) -> Result<()> {
    let commit = repo.find_commit(oid).context("failed to get commit")?;
    anyhow::ensure!(
        commit.parent_count() == 1,
        "fel stacks cannot contain merge commits"
    );

    let metadata = Metadata::new(&repo, commit.id())?;
    let (branch, force) = match &metadata.branch {
        Some(branch) => (branch.clone(), true),
        None => (format!("fel/{}/{}", branch_name, index), false),
    };
    let branch = pusher
        .push(commit.id(), branch, force)
        .await
        .map_err(|error| anyhow::anyhow!("failed to push branch: {}", error))?;

    // The parent branch is either the default branch, or the pushed branch of the
    // parent commit
    let base = if index == 0 {
        String::from("master")
    } else {
        pusher
            .wait(commit.parent_id(0)?)
            .await
            .map_err(|error| anyhow::anyhow!("failed to get parent branch: {}", error))?
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

    Ok(())
}

use std::sync::Arc;

use anyhow::{Context, Result};
use git2::{Oid, Repository};
use octocrab::Octocrab;

use crate::{gh::GHRepo, metadata::Metadata, push::Pusher};

pub struct CommitUpdater {
    octocrab: Arc<Octocrab>,
    branch_name: String,
    upstream_branch: String,
    gh_repo: GHRepo,
    pusher: Arc<Pusher>,
}

impl CommitUpdater {
    pub fn new(
        octocrab: Arc<Octocrab>,
        branch_name: &str,
        upstream_branch: &str,
        gh_repo: &GHRepo,
        pusher: Arc<Pusher>,
    ) -> Self {
        Self {
            octocrab,
            branch_name: branch_name.to_string(),
            upstream_branch: upstream_branch.to_string(),
            gh_repo: gh_repo.clone(),
            pusher,
        }
    }

    pub async fn update(&self, index: usize, repo: &Repository, oid: Oid) -> Result<()> {
        let commit = repo.find_commit(oid).context("failed to get commit")?;
        anyhow::ensure!(
            commit.parent_count() == 1,
            "fel stacks cannot contain merge commits"
        );

        let metadata = Metadata::new(&repo, commit.id())?;
        let (branch, force) = match &metadata.branch {
            Some(branch) => (branch.clone(), true),
            None => (format!("fel/{}/{}", self.branch_name, index), false),
        };
        let branch = self
            .pusher
            .push(commit.id(), branch, force)
            .await
            .map_err(|error| anyhow::anyhow!("failed to push branch: {}", error))?;

        // The parent branch is either the default branch, or the pushed branch of the
        // parent commit
        let base = if index == 0 {
            self.upstream_branch.clone()
        } else {
            self.pusher
                .wait(commit.parent_id(0)?)
                .await
                .map_err(|error| anyhow::anyhow!("failed to get parent branch: {}", error))?
        };

        let pr = match metadata.pr {
            None => {
                tracing::debug!(
                    owner = self.gh_repo.owner,
                    repo = self.gh_repo.repo,
                    branch,
                    base,
                    "creating PR"
                );
                self.octocrab
                    .pulls(&self.gh_repo.owner, &self.gh_repo.repo)
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
                    owner = self.gh_repo.owner,
                    repo = self.gh_repo.repo,
                    base,
                    "amending PR"
                );
                self.octocrab
                    .pulls(&self.gh_repo.owner, &self.gh_repo.repo)
                    .update(pr)
                    .base(base)
                    .send()
                    .await
                    .context("failed to update pr")?
            }
        };

        let mut history = metadata.history.unwrap_or(Vec::new());
        history.push(oid.to_string());

        let metadata = Metadata {
            pr: Some(pr.number),
            branch: Some(branch),
            revision: Some(metadata.revision.unwrap_or(0) + 1),
            commit: Some(oid.to_string()),
            history: Some(history),
        };
        tracing::debug!(?metadata, ?commit, "updating commit metadata");
        metadata
            .write(repo, &commit)
            .context("failed to write commit metadata")?;

        Ok(())
    }
}

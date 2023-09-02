use std::sync::Arc;

use anyhow::{Context, Result};
use git2::Repository;
use octocrab::Octocrab;

use crate::{gh::GHRepo, metadata::Metadata, push::Pusher, stack::Commit};

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

    pub async fn update(&self, index: usize, repo: &Repository, c: &Commit) -> Result<()> {
        // If the commit didn't change, we don't need to update anything
        if c.metadata.commit == Some(c.id.to_string()) {
            return Ok(());
        }

        let commit = repo.find_commit(c.id).context("failed to get commit")?;
        anyhow::ensure!(
            commit.parent_count() == 1,
            "fel stacks cannot contain merge commits"
        );

        let (branch, force) = match &c.metadata.branch {
            Some(branch) => (branch.clone(), true),
            None => (format!("fel/{}/{}", self.branch_name, index), false),
        };
        let branch = self
            .pusher
            .push(commit.id(), branch, force)
            .await
            .context("failed to push branch")?;

        // The parent branch is either the default branch, or the pushed branch of the
        // parent commit
        let base = if index == 0 {
            self.upstream_branch.clone()
        } else {
            self.pusher
                .wait(commit.parent_id(0)?)
                .await
                .context("failed to get parent branch")?
        };

        let pr = match c.metadata.pr {
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

        if let Some(revision) = c.metadata.revision {
            if let Some(commit) = &c.metadata.commit {
                self.octocrab
                    .issues(&self.gh_repo.owner, &self.gh_repo.repo)
                    .create_comment(
                        pr.number,
                        format!(
                    "Updated to revision {} [view diff](https://github.com/{}/{}/compare/{}..{})",
                    revision,
                    &self.gh_repo.owner,
                    &self.gh_repo.repo,
                    commit,
                    c.id,
                ),
                    )
                    .await
                    .context("failed to post update comment")?;
            }
        }

        let mut history = c.metadata.history.clone().unwrap_or(Vec::new());
        history.push(c.id.to_string());

        let metadata = Metadata {
            pr: Some(pr.number),
            branch: Some(branch),
            revision: Some(c.metadata.revision.unwrap_or(0) + 1),
            commit: Some(c.id.to_string()),
            history: Some(history),
        };
        tracing::debug!(?metadata, ?commit, "updating commit metadata");
        metadata
            .write(repo, &commit)
            .context("failed to write commit metadata")?;

        Ok(())
    }
}

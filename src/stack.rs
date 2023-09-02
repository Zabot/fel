use anyhow::{Context, Result};
use git2::{BranchType, Oid, Repository, Sort};

use crate::metadata::Metadata;

pub struct Commit {
    pub id: Oid,
    pub metadata: Metadata,
}

pub struct Stack {
    commits: Vec<Commit>,
    name: String,
}

impl Stack {
    pub fn new(repo: &Repository, default_upstream: &str) -> Result<Self> {
        // Find the local HEAD
        let head = repo.head().context("failed to get head")?;
        let head_commit = head.peel_to_commit().context("failed to get head commit")?;
        let branch_name = head.shorthand().context("invalid shorthand")?;
        tracing::debug!(branch_name, ?head_commit, "found HEAD");

        // Find the remote HEAD
        let default = repo
            .find_branch(default_upstream, BranchType::Remote)
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

        let commits: Vec<_> = walk
            .map(|oid| {
                let id = oid.context("failed to walk oid")?;
                Ok(Commit {
                    id,
                    metadata: Metadata::new(repo, id).context("failed to get metadata")?,
                })
            })
            .collect::<Result<_>>()
            .context("failed to get commits in stack")?;

        Ok(Self {
            commits,
            name: branch_name.to_string(),
        })
    }

    pub fn iter(&self) -> std::slice::Iter<Commit> {
        self.commits.iter()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

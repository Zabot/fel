use anyhow::{Context, Result};
use git2::{BranchType, Repository, Sort};

use crate::{commit::Commit, config::Config};

pub struct Stack {
    commits: Vec<Commit>,
    name: String,
    default_upstream: String,
}

impl Stack {
    pub fn new(repo: &Repository, config: &Config) -> Result<Self> {
        // Find the local HEAD
        let head = repo.head().context("failed to get head")?;
        let head_commit = head.peel_to_commit().context("failed to get head commit")?;
        let branch_name = head.shorthand().context("invalid shorthand")?.to_string();
        tracing::debug!(branch_name, ?head_commit, "found HEAD");

        // Find the remote HEAD
        let default = repo
            .find_branch(
                &format!("{}/{}", config.default_remote, config.default_upstream),
                BranchType::Remote,
            )
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
                let commit = repo.find_commit(id).context("failed to find commit")?;
                Commit::new(commit, repo)
            })
            .collect::<Result<_>>()
            .context("failed to get commits in stack")?;

        Ok(Self {
            commits,
            name: branch_name,
            default_upstream: config.default_upstream.clone(),
        })
    }

    /// Returns true if this stack does not have a branch associated with it
    pub fn is_detached(&self) -> bool {
        self.name == "HEAD"
    }

    /// Create a new branch with the same head as this stack
    pub fn dev_branch(&mut self, repo: &Repository) -> Result<()> {
        let head_commit = self.commits.first().context("no commits")?;
        let head_commit = repo
            .find_commit(head_commit.id())
            .context("find head commit")?;
        self.name = format!("dev-{}", &head_commit.id().to_string()[..4]);
        let branch = repo.branch(&self.name, &head_commit, false)?;
        let branch = branch.into_reference();
        let refname = branch.name().context("branch name not utf-8")?;
        repo.set_head(refname)?;

        Ok(())
    }

    pub fn iter(&self) -> std::slice::Iter<Commit> {
        self.commits.iter()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn upstream(&self) -> &str {
        &self.default_upstream
    }

    pub fn len(&self) -> usize {
        self.commits.len()
    }
}

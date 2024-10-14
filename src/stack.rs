use anyhow::{Context, Result};
use git2::{BranchType, Repository, Sort};

use crate::{commit::Commit, config::Config};

pub struct Stack {
    commits: Vec<Commit>,
    name: String,
    default_upstream: String,
}

impl Stack {
    /// Create a new stack from the current state of `repo`. The top of the stack is
    /// assumed to be the current HEAD, and the bottom of the stack is the first
    /// commit that is not found in the history of the remote branch.
    #[tracing::instrument(skip_all)]
    pub fn new(repo: &Repository, config: &Config) -> Result<Self> {
        // Find the local HEAD
        let head = repo.head().context("failed to get head")?;
        let head_commit = head.peel_to_commit().context("failed to get head commit")?;
        let branch_name = head
            .shorthand()
            .context("shorthand was not utf-8")?
            .to_string();
        tracing::debug!(branch_name, ?head_commit, "found HEAD");

        // Find the remote HEAD
        // TODO It would be great to do this smarter then a static config
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

        let commits = walk
            .map(|oid| {
                let id = oid.context("failed to walk oid")?;
                let commit = repo.find_commit(id).context("failed to find commit")?;
                Commit::new(commit, repo)
            })
            .collect::<Result<_>>()
            .context("failed to get commits in stack")?;

        tracing::debug!(?commits, "found commits");
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
    #[tracing::instrument(skip_all)]
    pub fn dev_branch(&mut self, repo: &Repository) -> Result<()> {
        let stack_top = self.iter().last().context("no commits")?;
        let top_commit = repo
            .find_commit(stack_top.id())
            .context("find head commit")?;

        self.name = format!("dev-{}", &top_commit.id().to_string()[..4]);
        tracing::debug!(
            branch = self.name,
            commit = ?top_commit.id(),
            "creating dev branch"
        );
        let branch = repo
            .branch(&self.name, &top_commit, false)
            .context("failed to create dev branch")?;

        tracing::debug!(branch = self.name, "checking out dev branch");
        let branch = branch.into_reference();
        let refname = branch.name().context("branch name not utf-8")?;
        repo.set_head(refname).context("checkout failed")?;

        Ok(())
    }

    /// Iterate over the commits in this stack, starting from the bottom
    /// and ending at the tip
    pub fn iter(&self) -> std::slice::Iter<Commit> {
        self.commits.iter()
    }

    /// Get the name of this stack
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the name of the upstream branch this stack
    /// is being compared against
    pub fn upstream(&self) -> &str {
        &self.default_upstream
    }

    /// Get the number of commits in the stack
    pub fn len(&self) -> usize {
        self.commits.len()
    }
}

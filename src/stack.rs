use ansi_term::Colour::Yellow;
use ansi_term::Style;
use anyhow::{Context, Result};
use git2::{BranchType, Oid, Repository, Sort};

use crate::{config::Config, metadata::Metadata};

pub struct Commit {
    pub id: Oid,
    pub metadata: Metadata,
}

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
        let mut branch_name = head.shorthand().context("invalid shorthand")?.to_string();

        // If there current HEAD is not a branch, create one and switch to it
        if config.auto_create_branches && &branch_name == "HEAD" {
            branch_name = format!("dev-{}", &head_commit.id().to_string()[..4]);
            let branch = repo.branch(&branch_name, &head_commit, false)?;
            let branch = branch.into_reference();
            let refname = branch.name().context("branch name not utf-8")?;
            repo.set_head(refname)?;
        }
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
                Ok(Commit {
                    id,
                    metadata: Metadata::new(repo, id).context("failed to get metadata")?,
                })
            })
            .collect::<Result<_>>()
            .context("failed to get commits in stack")?;

        Ok(Self {
            commits,
            name: branch_name,
            default_upstream: config.default_upstream.clone(),
        })
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

    pub fn render<F>(&self, color: bool, display: F) -> String
    where
        F: Fn(&Commit) -> String,
    {
        // TODO Thisd colorization stuff feels like it could be done a bit better
        let structure_style = if color {
            Yellow.normal()
        } else {
            Style::default()
        };

        let commit_marker = structure_style.paint("*").to_string();

        let mut nodes: Vec<_> = self
            .commits
            .iter()
            .rev()
            .map(|commit| format!("{commit_marker} {}", display(commit)))
            .collect();

        nodes.insert(
            0,
            structure_style
                .paint(format!("* {}", self.name))
                .to_string(),
        );
        nodes.push(
            structure_style
                .paint(format!("* {}", self.default_upstream))
                .to_string(),
        );

        nodes.join("\n")
    }
}

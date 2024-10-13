use anyhow::{Context, Result};
use git2::{Oid, Repository};

use crate::metadata::Metadata;

#[derive(Clone)]
pub struct Commit {
    pub metadata: Metadata,
    pub title: String,
    pub body: String,
    id: Oid,
    parent: Oid,
}

impl Commit {
    pub fn new<'repo>(commit: git2::Commit<'repo>, repo: &'repo Repository) -> Result<Commit> {
        let parent = commit.parent_id(0).context("get parent")?;
        Ok(Commit {
            metadata: Metadata::new(repo, &commit).context("failed to get metadata")?,
            title: commit.summary().context("summary not utf8")?.to_string(),
            body: commit.body().unwrap_or("body not utf8").to_string(),
            id: commit.id(),
            parent,
        })
    }

    //pub async fn push(&self, pusher: &BatchedPusher, default_branch_name: String) -> Result<()> {
    //let branch_name = self.metadata.branch.clone().unwrap_or(default_branch_name);
    //let force = self.metadata.branch.is_some();
    //let mut info = pusher.push(self.commit.id(), branch_name, force);

    //while let Ok(message) = info.recv().await {
    //tracing::info!(?message, "push result");
    //}

    //Ok(())
    //}

    pub fn id(&self) -> Oid {
        self.id
    }

    pub fn parent(&self) -> &Oid {
        &self.parent
    }
}

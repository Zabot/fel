use std::fmt::Debug;

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

impl Debug for Commit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.id.fmt(f)
    }
}

impl Commit {
    pub fn new<'repo>(commit: git2::Commit<'repo>, repo: &'repo Repository) -> Result<Commit> {
        let parent = commit.parent_id(0).context("get parent")?;
        Ok(Commit {
            metadata: Metadata::new(repo, commit.id()).context("failed to get metadata")?,
            title: commit.summary().context("summary not utf8")?.to_string(),
            body: commit.body().unwrap_or("body not utf8").to_string(),
            id: commit.id(),
            parent,
        })
    }

    pub fn id(&self) -> Oid {
        self.id
    }

    pub fn parent(&self) -> &Oid {
        &self.parent
    }
}

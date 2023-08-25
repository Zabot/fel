use anyhow::{Context, Result};
use git2::{Oid, Repository};

pub const NOTE_REF: &str = "refs/notes/fel";

#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct Metadata {
    pub branch: Option<String>,
    pub pr: Option<String>,
}

pub struct Commit {
    pub id: Oid,
    metadata: Metadata,
}

impl Commit {
    pub fn new(repo: &Repository, commit: Oid) -> Result<Self> {
        tracing::debug!(?commit, "walking tree");

        let note = repo.find_note(Some(NOTE_REF), commit);

        // check if this commit has a note already
        let metadata = match note {
            Ok(note) => {
                let metadata: Metadata =
                    toml::from_str(note.message().context("invalid note string")?)
                        .context("failed to parse metadata")?;
                tracing::debug!(?metadata, "using existing metadata");
                metadata
            }
            Err(error) => {
                tracing::debug!(?error, "error reading fel note");
                Metadata::default()
            }
        };

        Ok(Self {
            id: commit,
            metadata,
        })
    }

    pub fn metadata(&mut self) -> &mut Metadata {
        &mut self.metadata
    }

    pub fn flush_metadata(&self, repo: &Repository) -> Result<()> {
        let metadata =
            toml::to_string_pretty(&self.metadata).context("failed to serialize metadata")?;
        let sig = repo.signature().context("failed to get signature")?;
        tracing::debug!(metadata, ?self.id, "writing note");
        repo.note(&sig, &sig, Some(NOTE_REF), self.id, &metadata, true)
            .context("failed to create note")?;
        Ok(())
    }
}

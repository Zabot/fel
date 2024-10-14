use anyhow::{Context, Result};
use git2::{Oid, Repository};

pub const NOTE_REF: &str = "refs/notes/fel";

// TODO Maybe use protobuf here? Not sure it's any better then a struct
// full of options.
#[derive(serde::Serialize, serde::Deserialize, Debug, Default, Clone)]
pub struct Metadata {
    pub branch: Option<String>,
    pub pr: Option<u64>,
    pub revision: Option<u32>,
    pub commit: Option<String>,
    pub history: Option<Vec<String>>,
    pub pr_url: Option<String>,
}

impl Metadata {
    /// Attempt to fetch the metadata associted with a `commit` from the
    /// git notes in `repo`.
    #[tracing::instrument(skip(repo))]
    pub fn new(repo: &Repository, commit: Oid) -> Result<Self> {
        tracing::debug!("searching for note");

        let note = repo.find_note(Some(NOTE_REF), commit);

        // check if this commit has a note already
        let metadata = match note {
            Ok(note) => {
                let metadata: Metadata =
                    toml::from_str(note.message().context("note is not utf8")?)
                        .context("note is not valid toml")?;

                tracing::debug!(?metadata, "found metadata for commit");
                metadata
            }
            Err(error) => {
                tracing::debug!(?error, "no note for commit");
                Metadata::default()
            }
        };

        Ok(metadata)
    }

    /// Write the contents of this metadata back to `commit` in `repo`. If metadata already
    /// existed for that commit it will be overwritten.
    #[tracing::instrument(skip(repo))]
    pub fn write(&self, repo: &Repository, commit: Oid) -> Result<()> {
        let metadata = toml::to_string_pretty(&self).context("failed to serialize metadata")?;
        let sig = repo.signature().context("failed to get signature")?;

        tracing::debug!(metadata, ?commit, "writing metadata note");
        repo.note(&sig, &sig, Some(NOTE_REF), commit, &metadata, true)
            .context("failed to create note")?;
        Ok(())
    }
}

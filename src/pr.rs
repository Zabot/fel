use std::sync::Arc;

use anyhow::{Context, Result};
use octocrab::{models::pulls::PullRequest, pulls::PullRequestHandler, Octocrab};

use crate::gh::GHRepo;

pub struct PR {
    octocrab: Arc<Octocrab>,
    gh_repo: GHRepo,
}

const BODY_DELIM: &str = "[#]:fel";

#[derive(Debug)]
pub struct NewPr {
    pub title: String,
    pub body: String,
    pub base: String,
    pub branch: String,
}

#[derive(Default, Debug)]
pub struct PartialUpdate {
    pub title: Option<String>,
    pub body: Option<String>,
    pub footer: Option<String>,
    pub base: Option<String>,
}

fn join_footer(body: &str, footer: &str) -> String {
    format!("{}\n\n{BODY_DELIM}\n\n{}", body, footer)
}

fn split_footer(full: &str) -> (&str, &str) {
    let mut split = full.split(BODY_DELIM);
    let body = split.next().unwrap_or_default();
    let footer = split.next().unwrap_or_default();
    (body, footer)
}

impl PR {
    pub fn new(octocrab: Arc<Octocrab>, gh_repo: GHRepo) -> Self {
        Self { octocrab, gh_repo }
    }

    fn pulls(&self) -> PullRequestHandler {
        self.octocrab.pulls(&self.gh_repo.owner, &self.gh_repo.repo)
    }

    /// Get the pull request numbered `pr`
    #[tracing::instrument(skip(self))]
    pub async fn get(&self, pr: u64) -> Result<PullRequest> {
        tracing::debug!("getting PR");
        self.pulls().get(pr).await.context("failed to get pr")
    }

    /// Create a new pull request with `data`
    #[tracing::instrument(skip(self))]
    pub async fn create(&self, data: NewPr) -> Result<PullRequest> {
        tracing::debug!("creating PR");
        self.pulls()
            .create(&data.title, &data.branch, &data.base)
            .body(&data.body)
            .send()
            .await
            .context("failed to create pr")
    }

    /// Replace all of the contents of a PR with the specified data as though it was
    /// being created fresh.
    #[tracing::instrument(skip(self))]
    pub async fn replace(&self, pr: u64, footer: String, data: NewPr) -> Result<PullRequest> {
        tracing::debug!("replacing PR");
        let body = join_footer(&data.body, &footer);
        self.pulls()
            .update(pr)
            .title(&data.title)
            .base(&data.base)
            .body(&body)
            .send()
            .await
            .context("failed to update pr")
    }

    // Update `pr with only the fields that are not `None` in `data`
    #[tracing::instrument(skip(self))]
    pub async fn update(&self, pr: &PullRequest, data: PartialUpdate) -> Result<PullRequest> {
        tracing::debug!("updating PR");
        let (original_body, original_footer) = split_footer(pr.body.as_deref().unwrap_or_default());

        let new_body = match (data.body, data.footer) {
            (Some(body), Some(footer)) => Some(join_footer(&body, &footer)),
            (Some(body), None) => Some(join_footer(&body, original_footer)),
            (None, Some(footer)) => Some(join_footer(&original_body, &footer)),
            (None, None) => None,
        };

        let pulls = self.pulls();
        let mut builder = pulls.update(pr.number);

        // Infuriatingly `Option<String>` is not `impl Into<Option<impl Into<String>>>`, and there
        // is no other way to pass an option to the builder, despite the fact that in the request
        // these values are Option<String>.
        if let Some(base) = data.base {
            builder = builder.base(base)
        }

        if let Some(body) = new_body {
            builder = builder.body(body)
        }

        builder.send().await.context("failed to update pr")
    }
}

use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use futures::stream::{FuturesOrdered, StreamExt};
use git2::{Oid, Repository};
use octocrab::{models::pulls::PullRequest, Octocrab};
use parking_lot::RwLock;
use tokio::sync::Barrier;

use crate::{
    config::Config,
    gh::GHRepo,
    metadata::Metadata,
    push::Pusher,
    stack::{Commit, Stack},
};

pub enum Action {
    UpToDate(PullRequest),
    CreatedPR(PullRequest),
    UpdatedPR(PullRequest),
}

impl Action {
    pub fn pr(&self) -> &PullRequest {
        match self {
            Action::UpToDate(pr) => pr,
            Action::CreatedPR(pr) => pr,
            Action::UpdatedPR(pr) => pr,
        }
    }
}

pub struct CommitUpdater {
    octocrab: Arc<Octocrab>,
    gh_repo: GHRepo,
    pusher: Arc<Pusher>,

    prs: RwLock<HashMap<Oid, PullRequest>>,
}

const BODY_DELIM: &str = "[#]:fel";

impl CommitUpdater {
    pub fn new(octocrab: Arc<Octocrab>, gh_repo: &GHRepo, pusher: Arc<Pusher>) -> Self {
        Self {
            octocrab,
            gh_repo: gh_repo.clone(),
            pusher,
            prs: RwLock::new(HashMap::new()),
        }
    }

    pub async fn update_stack(
        &self,
        repo: &Repository,
        stack: &Stack,
        config: &Config,
    ) -> Result<HashMap<Oid, Action>> {
        let barrier = Barrier::new(stack.len());

        let futures: Result<FuturesOrdered<_>> = stack
            .iter()
            .enumerate()
            .map(|(i, commit)| Ok(self.update_commit(i, repo, commit, stack, &barrier, config)))
            .collect();
        let futures = futures.context("failed to generate futures")?;
        let futures = futures.collect::<Vec<_>>();
        let actions: Result<Vec<_>> = futures.await.into_iter().collect();
        let actions = actions.context("failed to update commit")?;
        Ok(stack
            .iter()
            .map(|c| c.id)
            .zip(actions.into_iter())
            .collect())
    }

    async fn update_commit(
        &self,
        index: usize,
        repo: &Repository,
        c: &Commit,
        stack: &Stack,
        barrier: &Barrier,
        config: &Config,
    ) -> Result<Action> {
        let commit = repo.find_commit(c.id).context("failed to get commit")?;
        anyhow::ensure!(
            commit.parent_count() == 1,
            "fel stacks cannot contain merge commits"
        );

        let (branch, already_exists) = match &c.metadata.branch {
            Some(branch) => (branch.clone(), true),
            None => {
                let branch_name = match config.use_indexed_branches {
                    true => format!("fel/{}/{index}", stack.name()),
                    false => format!("fel/{}/{}", stack.name(), &c.id.to_string()[..4]),
                };

                let branch_name = match config.branch_prefix.as_ref() {
                    Some(prefix) => format!("{prefix}/{branch_name}"),
                    None => branch_name,
                };

                (branch_name, false)
            }
        };
        let branch = self
            .pusher
            .push(commit.id(), branch, already_exists)
            .await
            .context("failed to push branch")?;

        // The parent branch is either the default branch, or the pushed branch of the
        // parent commit
        let base = if index == 0 {
            stack.upstream().to_string()
        } else {
            self.pusher
                .wait(commit.parent_id(0)?)
                .await
                .context("failed to get parent branch")?
        };

        let pr = match c.metadata.pr {
            Some(pr) => self
                .octocrab
                .pulls(&self.gh_repo.owner, &self.gh_repo.repo)
                .get(pr)
                .await
                .context("failed to get existing PR")?,

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
        };
        self.prs.write().insert(c.id, pr.clone());

        // TODO There is other stuff we could be doing while we're waiting for the prs to come in
        // from everywhere else, if we restructure this a bit we could probably aovid every having
        // to block at this barrier.
        // Wait for all of the other PRs to come in
        barrier.wait().await;

        // We always have to ammend the PR message because we need to know the number of every PR
        // before we can add the footer message.
        let old = &pr.body.clone().unwrap_or("".to_string());
        let body = match old.split_once(BODY_DELIM) {
            None => old,
            Some((body, _)) => body,
        };

        // Serialize the stack to a footer
        let tree = stack.render(false, |c| {
            let guard = self.prs.read();
            let Some(pr) = guard.get(&c.id) else {
                return "unknown".to_string()
            };

            format!(
                "<a href=\"{}\">#{} {}</a>",
                pr.number,
                pr.number,
                pr.title.clone().unwrap_or("".to_string())
            )
        });
        let body = format!(
            "{body}

{BODY_DELIM}

---
<pre>
{tree}
</pre>
This diff is part of a [fel stack](https://github.com/zabot/fel).
"
        );

        tracing::debug!(
            pr.number,
            owner = self.gh_repo.owner,
            repo = self.gh_repo.repo,
            base,
            body,
            "amending PR"
        );
        self.octocrab
            .pulls(&self.gh_repo.owner, &self.gh_repo.repo)
            .update(pr.number)
            .base(base)
            .body(body)
            .send()
            .await
            .context("failed to update pr")?;

        // If the commit wasn't changed since last time, we don't need to update anything else
        if c.metadata.commit == Some(c.id.to_string()) {
            return Ok(Action::UpToDate(pr));
        }

        // Make a comment with the diff since the last submit
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

        if c.metadata.pr.is_none() {
            Ok(Action::CreatedPR(pr))
        } else {
            Ok(Action::UpdatedPR(pr))
        }
    }
}

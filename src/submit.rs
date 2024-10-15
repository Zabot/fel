use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use ansi_term::Colour::{Green, Red, Yellow};
use ansi_term::{Color, Style};
use anyhow::{Context, Result};
use futures::{stream::FuturesUnordered, TryStreamExt};
use git2::{Oid, Remote, Repository};
use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
use tokio::sync::Notify;

use crate::auth;
use crate::await_map::AwaitMap;
use crate::commit::Commit;
use crate::config::Config;
use crate::metadata::Metadata;
use crate::pr::{NewPr, PartialUpdate, PR};
use crate::push::BatchedPusher;
use crate::render::{RenderInfo, RenderStore, TeraRender};
use crate::stack::Stack;

pub struct Submit {
    use_indexed_branches: bool,
    branch_prefix: Option<String>,
    authoritative_commits: bool,

    pulls: PR,
    stack: Stack,
    pusher: BatchedPusher,
    render_store: RenderStore<TeraRender>,
    branch_names: AwaitMap<Oid, String>,
}

struct SubmitProgress {
    oid: Oid,
    title: String,
    pr_num: Option<u64>,
    pr_title: Option<String>,
    pr_url: Option<String>,

    pb: ProgressBar,
}

impl SubmitProgress {
    fn new(commit: &Commit, pb: ProgressBar) -> Result<Self> {
        let progress = Self {
            oid: commit.id(),
            title: commit.title.clone(),
            pr_num: commit.metadata.pr,
            pr_title: None,
            pr_url: commit.metadata.pr_url.clone(),
            pb,
        };
        progress.update()?;
        Ok(progress)
    }

    fn update(&self) -> Result<()> {
        self.do_update(Yellow, true)
    }

    fn set_message(&self, msg: impl Into<Cow<'static, str>>) {
        self.pb.set_message(msg)
    }

    fn finish(&self, message: impl Into<Cow<'static, str>>, color: Color) -> Result<()> {
        self.do_update(color, false)?;
        self.pb.finish_with_message(message);
        Ok(())
    }

    fn do_update(&self, color: Color, show_spinner: bool) -> Result<()> {
        let bullet = Yellow.paint(format!(
            "* {}",
            self.pr_num
                .map(|pr| format!("#{pr}"))
                .unwrap_or(self.oid.to_string()[..8].to_string())
        ));

        let url = Style::default()
            .dimmed()
            .paint(self.pr_url.clone().unwrap_or_default());
        self.pb.set_prefix(format!(
            "{} {url}",
            self.pr_title.as_ref().unwrap_or(&self.title)
        ));

        let spinner = if show_spinner { "{spinner} " } else { "" };

        let style = ProgressStyle::default_spinner()
            .template(&format!(
                "{bullet} {} {{prefix}}",
                color.paint(format!("[{spinner}{{msg}}]")),
            ))
            .context("invalid style")?;

        self.pb.set_style(style);

        Ok(())
    }
}

impl Submit {
    pub fn new(stack: Stack, pulls: PR, config: &Config) -> Self {
        let pusher = BatchedPusher::default();

        let render = TeraRender::new().unwrap();
        let render_store = RenderStore::new(render);

        Self {
            pusher,
            use_indexed_branches: config.submit.use_indexed_branches,
            branch_prefix: config.submit.branch_prefix.clone(),
            pulls,
            authoritative_commits: config.submit.authoritative_commits,
            branch_names: AwaitMap::new(),
            render_store,
            stack,
        }
    }

    pub async fn run(
        self: &Arc<Self>,
        remote: &mut Remote<'_>,
        repo: &Repository,
        progress: &MultiProgress,
    ) -> Result<()> {
        let notify = Arc::new(Notify::new());

        let tasks: FuturesUnordered<_> = self
            .stack
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, commit)| {
                // TODO We actually may not want to do this, since it could confuse
                // GH if we create a PR on a base branch before we update the base
                // branch.
                if let Some(branch) = commit.metadata.branch.clone() {
                    self.branch_names.insert(commit.id(), branch)
                }

                // If commit messages are authoritative we don't need to wait for GH to tell us
                // information about the commit
                if self.authoritative_commits {
                    if let Some(pr) = commit.metadata.pr {
                        self.render_store.record(
                            commit.id(),
                            RenderInfo {
                                title: commit.title.clone(),
                                number: pr,
                                commit: commit.id().to_string(),
                            },
                        );
                    }
                }

                // Setup the spinner
                let pb = progress.insert(0, ProgressBar::new_spinner());
                pb.enable_steady_tick(Duration::from_millis(100));
                let mut progress = SubmitProgress::new(&commit, pb).unwrap();
                progress.set_message("connecting to remote");

                let notify = notify.clone();
                let submit = self.clone();
                tokio::spawn(async move {
                    // Wait for the remote connection before proceding
                    notify.notified().await;

                    let result = submit.submit_commit(commit, index, &mut progress).await;

                    if result.is_err() {
                        progress.finish("failed", Red)?;
                    }
                    result
                })
            })
            .collect();

        let upstream_pb = progress.insert_from_back(
            0,
            ProgressBar::new_spinner().with_finish(ProgressFinish::AndLeave),
        );
        let style = ProgressStyle::default_spinner()
            .template("{prefix} {spinner} {msg}")
            .context("invalid style")?;
        upstream_pb.enable_steady_tick(Duration::from_millis(100));
        upstream_pb.set_style(style.clone());
        upstream_pb.set_prefix(
            Yellow
                .paint(format!("* {}", self.stack.upstream()))
                .to_string(),
        );

        let style = ProgressStyle::default_spinner()
            .template("{prefix} {msg}")
            .context("invalid style")?;
        let branch_pb = progress.insert(
            0,
            ProgressBar::new_spinner().with_finish(indicatif::ProgressFinish::AndLeave),
        );
        branch_pb.set_style(style);
        branch_pb.set_prefix(Yellow.paint(format!("* {}", self.stack.name())).to_string());

        upstream_pb.set_message("Connecting to remote");
        let mut conn = remote
            .connect_auth(git2::Direction::Push, Some(auth::callbacks()), None)
            .context("failed to connect to repo")?;
        notify.notify_waiters();

        upstream_pb.set_message("Pushing branches");
        self.pusher
            .wait_for(self.stack.len(), conn.remote())
            .await?;

        upstream_pb.set_message("Updating PRs");
        let results: Vec<_> = tasks.try_collect().await.context("failed to join")?;

        // Update all of the commit notes with the new metadata
        // We have to to this on this thread because Repository
        // is not thread safe.
        upstream_pb.set_message("Writing metadata");
        for result in results.into_iter() {
            let (id, metadata) = result.context("push failed")?;

            metadata
                .write(repo, id)
                .context("failed to write commit metadata")?;
        }

        upstream_pb.finish_with_message("");

        Ok(())
    }

    async fn submit_commit(
        &self,
        commit: Commit,
        index: usize,
        progress: &mut SubmitProgress,
    ) -> Result<(Oid, Metadata)> {
        // Figure out the branch name
        let force_push = commit.metadata.branch.is_some();
        let branch_name = commit.metadata.branch.clone().unwrap_or_else(|| {
            let branch_name = match self.use_indexed_branches {
                true => format!("fel/{}/{index}", &self.stack.name()),
                false => {
                    format!(
                        "fel/{}/{}",
                        &self.stack.name(),
                        &commit.id().to_string()[..4]
                    )
                }
            };

            match self.branch_prefix.as_ref() {
                Some(prefix) => format!("{prefix}/{branch_name}"),
                None => branch_name,
            }
        });

        // Push the branch to remote
        progress.set_message("pushing branch");
        self.pusher
            .push(commit.id(), branch_name.clone(), force_push)
            .await
            .context("push branch")?;

        self.branch_names.insert(commit.id(), branch_name.clone());

        // Now we need to figure out the branch name of the parent
        let base_branch = if index == 0 {
            self.stack.upstream().to_string()
        } else {
            // TODO We may need to make sure that the parent branch was actually
            // finished pushing before we proceed here. Even if the branch name
            // was cached in the commit metadata if we update the base before
            // we push the branch, github may get confused.
            self.branch_names.get(commit.parent()).await
        };

        // Now we can create the PR
        let created_pr;
        let pr_data = NewPr {
            base: base_branch.clone(),
            body: commit.body.clone(),
            title: commit.title.clone(),
            branch: branch_name.clone(),
        };

        let pr = match commit.metadata.pr {
            // If the commit messages are authoritative we
            // don't need to bother fetching first, we can
            // just clobber everything.
            Some(pr) if self.authoritative_commits => {
                progress.set_message(format!("updating PR {pr}"));
                created_pr = false;

                let footer = self
                    .render_store
                    .render_stack(commit.id(), &self.stack)
                    .await?;

                self.pulls
                    .replace(pr, footer, pr_data)
                    .await
                    .context("failed to update existing PR")?
            }
            Some(pr) => {
                progress.set_message(format!("fetching PR {pr}"));
                created_pr = false;

                self.pulls
                    .get(pr)
                    .await
                    .context("failed to get existing PR")?
            }
            None => {
                progress.set_message("creating PR");
                created_pr = true;

                self.pulls
                    .create(pr_data)
                    .await
                    .context("failed to create PR")?
            }
        };

        progress.pr_num = Some(pr.number);
        progress.pr_title = pr.title.clone();
        progress.pr_url = pr.html_url.as_ref().map(|url| url.to_string());
        progress.update()?;
        self.render_store.record(
            commit.id(),
            RenderInfo {
                number: pr.number,
                title: pr.title.clone().unwrap_or_default(),
                commit: commit.id().to_string(),
            },
        );

        // If the commit messages are authoritative we don't need to do this second update step
        // (unless we had to create the PR in the first place) because we already wrote the
        // footer when we updated.
        if !self.authoritative_commits || created_pr {
            // We may not have known the pr numbers of every commit in the stack until after
            // we created all the prs, so now we need to update the prs with the footer
            // We also may need to update the base branch to restack the prs
            let footer = self
                .render_store
                .render_stack(commit.id(), &self.stack)
                .await?;

            progress.set_message("updating PR footer");
            self.pulls
                .update(
                    &pr,
                    PartialUpdate {
                        base: Some(base_branch.clone()),
                        footer: Some(footer.clone()),
                        ..Default::default()
                    },
                )
                .await
                .context("failed to update existing PR")?;
        }

        let mut history = commit.metadata.history.clone().unwrap_or(Vec::new());
        if Some(commit.id().to_string()) == commit.metadata.commit {
            progress.finish("up to date", Green)?;
        } else {
            if created_pr {
                progress.finish("created", Yellow)?;
            } else {
                progress.finish("updated", Yellow)?;
            }
            history.push(commit.id().to_string());
        }

        let metadata = Metadata {
            pr: Some(pr.number),
            branch: Some(branch_name),
            revision: Some(commit.metadata.revision.unwrap_or(0) + 1),
            commit: Some(commit.id().to_string()),
            history: Some(history),
            pr_url: Some(pr.html_url.map(|url| url.to_string()).unwrap_or_default()),
        };

        Ok::<_, anyhow::Error>((commit.id(), metadata))
    }
}

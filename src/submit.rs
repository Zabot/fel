use ansi_term::Colour::{Green, Red, Yellow};
use ansi_term::{Color, Style};
use anyhow::{Context, Result};
use futures::{stream::FuturesUnordered, TryStreamExt};
use git2::{Oid, Remote, Repository};
use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
use octocrab::pulls::PullRequestHandler;
use octocrab::Octocrab;
use parking_lot::RwLock;
use tera::Tera;
use tokio::sync::{watch, Notify};

use crate::auth;
use crate::commit::Commit;
use crate::config::Config;
use crate::gh::GHRepo;
use crate::metadata::Metadata;
use crate::push::BatchedPusher;
use crate::stack::Stack;

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

const BODY_DELIM: &str = "[#]:fel";

#[derive(serde::Serialize, Clone)]
struct PrInfo {
    number: u64,
    title: String,
}

struct Submit {
    octocrab: Arc<Octocrab>,
    gh_repo: GHRepo,

    use_indexed_branches: bool,
    branch_prefix: Option<String>,
    stack_name: String,
    stack_upstream: String,

    pusher: BatchedPusher,
    footer_rx: watch::Receiver<Option<String>>,

    branch_names: RwLock<HashMap<git2::Oid, watch::Receiver<Option<String>>>>,
    pr_info: RwLock<HashMap<git2::Oid, watch::Receiver<Option<PrInfo>>>>,
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
    fn pulls(&self) -> PullRequestHandler {
        self.octocrab.pulls(&self.gh_repo.owner, &self.gh_repo.repo)
    }

    async fn submit_commit(
        &self,
        commit: Commit,
        index: usize,
        progress: &mut SubmitProgress,
        branch_name_tx: watch::Sender<Option<String>>,
        pr_info_tx: watch::Sender<Option<PrInfo>>,
    ) -> Result<(Oid, Metadata)> {
        // Figure out the branch name
        let force_push = commit.metadata.branch.is_some();
        let branch_name = commit.metadata.branch.clone().unwrap_or_else(|| {
            let branch_name = match self.use_indexed_branches {
                true => format!("fel/{}/{index}", &self.stack_name),
                false => {
                    format!("fel/{}/{}", &self.stack_name, &commit.id().to_string()[..4])
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

        branch_name_tx.send_replace(Some(branch_name.clone()));

        // Now we need to figure out the branch name of the parent
        let base_branch = if index == 0 {
            self.stack_upstream.clone()
        } else {
            let mut rx = self
                .branch_names
                .read()
                .get(commit.parent())
                .context("parent commit unknown")?
                .clone();

            let branch = rx
                .wait_for(|branch| branch.is_some())
                .await
                .context("wait for parent branch")?;

            branch.clone().context("branch was none")?
        };

        // Now we can create the PR
        let created_pr;
        let pr = match commit.metadata.pr {
            Some(pr) => {
                progress.set_message(format!("fetching PR {pr}"));
                created_pr = false;
                self.pulls()
                    .get(pr)
                    .await
                    .context("failed to get existing PR")?
            }
            None => {
                progress.set_message("creating PR");
                created_pr = true;
                tracing::debug!(branch_name, base_branch, "creating PR");
                self.pulls()
                    .create(&commit.title, &branch_name, &base_branch)
                    .body(&commit.body)
                    .send()
                    .await
                    .context("failed to create pr")?
            }
        };

        progress.pr_num = Some(pr.number);
        progress.pr_title = pr.title.clone();
        progress.pr_url = pr.html_url.as_ref().map(|url| url.to_string());
        progress.update()?;
        pr_info_tx.send_replace(Some(PrInfo {
            number: pr.number,
            title: pr.title.unwrap_or_default(),
        }));

        // We may not have known the pr numbers of every commit in the stack until after
        // we created all the prs, so now we need to update the prs with the footer
        // We also may need to update the base branch to restack the prs
        // TODO If the commit messages are authoritaive we can skip this step and do
        // this all with only one round trip
        let footer = self
            .footer_rx
            .clone()
            .wait_for(|footer| footer.is_some())
            .await
            .context("wait for footer")?
            .clone()
            .context("footer was none")?;

        let original_body = pr.body.clone().unwrap_or_default();
        let original_body = original_body.split(BODY_DELIM).next().unwrap_or_default();

        let body = format!("{original_body}\n\n{BODY_DELIM}\n\n{footer}");

        progress.set_message("updating PR footer");
        self.pulls()
            .update(pr.number)
            .base(base_branch)
            .body(body)
            .send()
            .await
            .context("failed to update pr")?;

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

        // TODO Update the metadata after the commit
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

    fn new(
        stack: &Stack,
        octocrab: Arc<Octocrab>,
        gh_repo: &GHRepo,
        config: &Config,
        footer_rx: watch::Receiver<Option<String>>,
    ) -> Self {
        let pusher = BatchedPusher::default();
        let branch_names = RwLock::new(HashMap::new());
        let pr_info = RwLock::new(HashMap::new());

        Self {
            pusher,
            use_indexed_branches: config.submit.use_indexed_branches,
            branch_prefix: config.submit.branch_prefix.clone(),
            octocrab,
            gh_repo: gh_repo.clone(),
            stack_name: stack.name().to_string(),
            stack_upstream: stack.upstream().to_string(),
            branch_names,
            pr_info,
            footer_rx,
        }
    }

    async fn render_footer(
        &self,
        commits: Vec<Oid>,
        footer_tx: watch::Sender<Option<String>>,
    ) -> Result<()> {
        let mut prs = Vec::new();
        for id in commits {
            let mut info = self
                .pr_info
                .read()
                .get(&id)
                .with_context(|| format!("missing commit: {id}"))?
                .clone();

            prs.insert(
                0,
                info.wait_for(|pr| pr.is_some())
                    .await
                    .context("await pr info")?
                    .clone()
                    .context("info is none")?,
            );
        }

        // TODO This is totally overkill
        let mut tera = Tera::default();
        tera.add_raw_template("footer.html", include_str!("../templates/footer.html"))?;
        let mut context = tera::Context::new();
        context.insert("prs", &prs);
        context.insert("stack_name", &self.stack_name);
        context.insert("upstream", &self.stack_upstream);
        let footer = tera
            .render("footer.html", &context)
            .context("render footer")?;
        tracing::debug!(footer, "rendered footer");

        footer_tx.send_replace(Some(footer));
        Ok::<_, anyhow::Error>(())
    }
}

pub async fn submit(
    stack: &Stack,
    remote: &mut Remote<'_>,
    octocrab: Arc<Octocrab>,
    gh_repo: &GHRepo,
    repo: &Repository,
    config: &Config,
) -> Result<()> {
    let progress = MultiProgress::new();
    let (footer_tx, footer_rx) = watch::channel(None);

    let submit = Arc::new(Submit::new(stack, octocrab, gh_repo, config, footer_rx));

    let notify = Arc::new(Notify::new());

    let tasks: FuturesUnordered<_> = stack
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, commit)| {
            let (branch_name_tx, branch_name_rx) = watch::channel(commit.metadata.branch.clone());
            submit
                .branch_names
                .write()
                .insert(commit.id(), branch_name_rx);

            let (pr_info_tx, pr_info_rx) = watch::channel(None);
            submit.pr_info.write().insert(commit.id(), pr_info_rx);

            // Setup the spinner
            let pb = progress.insert(0, ProgressBar::new_spinner());
            pb.enable_steady_tick(Duration::from_millis(100));
            let mut progress = SubmitProgress::new(&commit, pb).unwrap();
            progress.set_message("connecting to remote");

            let notify = notify.clone();
            let submit = submit.clone();
            tokio::spawn(async move {
                // Wait for the remote connection before proceding
                notify.notified().await;

                let result = submit
                    .submit_commit(commit, index, &mut progress, branch_name_tx, pr_info_tx)
                    .await;

                if result.is_err() {
                    progress.finish("failed", Red)?;
                }
                result
            })
        })
        .collect();

    tokio::spawn({
        let progress = progress.clone();
        let submit = submit.clone();
        let commits = stack.iter().map(|c| c.id()).collect();
        async move {
            if let Err(error) = submit.render_footer(commits, footer_tx).await {
                progress
                    .println(format!("failed to render footer: {:?}", error))
                    .ok();
            }
        }
    });

    let upstream_pb = progress.insert_from_back(
        0,
        ProgressBar::new_spinner().with_finish(ProgressFinish::AndLeave),
    );
    let style = ProgressStyle::default_spinner()
        .template("{prefix} {spinner} {msg}")
        .context("invalid style")?;
    upstream_pb.enable_steady_tick(Duration::from_millis(100));
    upstream_pb.set_style(style.clone());
    upstream_pb.set_prefix(Yellow.paint(format!("* {}", stack.upstream())).to_string());

    let style = ProgressStyle::default_spinner()
        .template("{prefix} {msg}")
        .context("invalid style")?;
    let branch_pb = progress.insert(
        0,
        ProgressBar::new_spinner().with_finish(indicatif::ProgressFinish::AndLeave),
    );
    branch_pb.set_style(style);
    branch_pb.set_prefix(Yellow.paint(format!("* {}", stack.name())).to_string());

    upstream_pb.set_message("Connecting to remote");
    let mut conn = remote
        .connect_auth(git2::Direction::Push, Some(auth::callbacks()), None)
        .context("failed to connect to repo")?;
    notify.notify_waiters();

    upstream_pb.set_message("Pushing branches");
    submit.pusher.wait_for(stack.len(), conn.remote()).await?;

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

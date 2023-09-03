use ansi_term::Colour::{Green, Yellow};
use ansi_term::Style;
use anyhow::{Context, Result};
use git2::{Remote, Repository};
use octocrab::Octocrab;

use crate::auth;
use crate::gh::GHRepo;
use crate::push::Pusher;
use crate::stack::Stack;
use crate::update::{Action, CommitUpdater};

use std::sync::Arc;

pub async fn submit(
    stack: &Stack,
    remote: &mut Remote<'_>,
    gh_repo: &GHRepo,
    octocrab: Arc<Octocrab>,
    repo: &Repository,
) -> Result<()> {
    tracing::debug!(remote = remote.name(), "connecting to remote");
    let mut conn = remote
        .connect_auth(git2::Direction::Push, Some(auth::callbacks()), None)
        .context("failed to connect to repo")?;
    tracing::debug!(connected = conn.connected(), "remote connected");

    let pusher = Arc::new(Pusher::new());

    let updater = CommitUpdater::new(octocrab.clone(), gh_repo, pusher.clone());
    let update = updater.update_stack(repo, stack);
    let send = pusher.send(stack.len(), conn.remote());

    let (actions, _) = futures::try_join!(update, send).context("failed to await tasks")?;

    println!(
        "{}",
        stack.render(true, |c| {
            let Some(action) = actions.get(&c.id) else {
                return format!("{} unknown", c.id);
            };
            let pr = action.pr();

            let url = Style::default()
                .dimmed()
                .paint(pr.html_url.as_ref().map(|url| url.as_str()).unwrap_or(""));

            let pr_title = format!(
                "#{} {} {}",
                pr.number,
                pr.title.clone().unwrap_or("".to_string()),
                url,
            );

            let status = match action {
                Action::UpToDate(_) => Green.paint("[up to date]"),
                Action::UpdatedPR(_) => Yellow.paint("[updated]"),
                Action::CreatedPR(_) => Yellow.paint("[created]"),
            };
            format!("{status} {pr_title}")
        })
    );

    Ok(())
}

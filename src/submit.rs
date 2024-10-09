use ansi_term::Colour::{Green, Yellow};
use ansi_term::Style;
use anyhow::{Context, Result};
use git2::{Remote, Repository};
use indicatif::ProgressBar;
use octocrab::Octocrab;

use crate::auth;
use crate::gh::GHRepo;
use crate::push::Pusher;
use crate::stack::Stack;
use crate::update::{Action, CommitUpdater};

use std::sync::Arc;
use std::time::Duration;

pub async fn submit(
    stack: &Stack,
    remote: &mut Remote<'_>,
    gh_repo: &GHRepo,
    octocrab: Arc<Octocrab>,
    repo: &Repository,
) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.enable_steady_tick(Duration::from_millis(100));

    let remote_name = remote.name().unwrap_or("unnamed remote").to_string();
    tracing::debug!(remote_name, "connecting to remote");
    spinner.set_message(format!("Connecting to {remote_name}...",));

    let mut conn = remote
        .connect_auth(git2::Direction::Push, Some(auth::callbacks()), None)
        .context("failed to connect to repo")?;

    tracing::debug!(connected = conn.connected(), "remote connected");
    spinner.println(format!("Connected to {remote_name}",));

    let pusher = Arc::new(Pusher::new());

    spinner.set_message(format!("Updating stack..."));
    let updater = CommitUpdater::new(octocrab.clone(), gh_repo, pusher.clone());
    let update = updater.update_stack(repo, stack);
    let send = pusher.send(stack.len(), conn.remote());
    spinner.println("Updated stack");

    spinner.set_message(format!("Updating PRs..."));
    let (actions, _) = futures::try_join!(update, send).context("failed to await tasks")?;
    spinner.println("Updated PRs");

    spinner.finish_and_clear();
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

use anyhow::{Context, Result};
use git2::BranchType;
use git2::Config;
use git2::PushOptions;
use git2::RemoteCallbacks;
use git2::Repository;
use git2::Sort;

mod auth;
mod metadata;
use metadata::Commit;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // rewriteRef makes amends and rebases copy the notes from the orginal commit
    let config = Config::open_default().context("failed to open config")?;
    let rewrite_ref = config
        .entries(Some("notes.rewriteref"))
        .context("failed to get notes.rewriteRef")?;

    let mut found = false;
    rewrite_ref.for_each(|entry| {
        if entry.value() == Some("refs/notes/fel") {
            found = true;
        }
    })?;
    anyhow::ensure!(
        found,
        "notes.rewriteRef must include 'refs/notes/fel' for fel to work properly"
    );

    // Find the local HEAD
    let repo = Repository::discover("test").context("failed to open repo")?;
    let head = repo.head().context("failed to get head")?;
    let head_commit = head.peel_to_commit().context("failed to get head commit")?;
    let branch_name = head.shorthand().context("invalid shorthand")?;
    tracing::debug!(branch_name, ?head_commit, "found HEAD");

    // Find the remote HEAD
    let default = repo
        .find_branch("master", BranchType::Local)
        .context("failed to find default branch")?;
    let default_commit = default
        .get()
        .peel_to_commit()
        .context("failed to get default commit")?;
    tracing::debug!(?default_commit, "found default HEAD");

    // Calculate the first common ancestor
    let merge_base = repo
        .merge_base(default_commit.id(), head_commit.id())
        .context("failed to locate merge base")?;
    tracing::debug!(?merge_base, "found merge base");

    let mut walk = repo.revwalk().context("failed to create revwalk")?;
    walk.push(head_commit.id())
        .context("failed to add commit to revwalk")?;
    walk.hide(merge_base).context("failed to hide revwalk")?;
    walk.set_sorting(Sort::REVERSE)
        .context("failed to set sorting")?;

    // Gather up all of the commits, reading their existing metadata
    let commits: Result<Vec<Commit>> = walk.map(|commit| Commit::new(&repo, commit?)).collect();
    let mut commits = commits?;

    tracing::debug!("generating push refspecs");
    let refspecs: Vec<String> = commits
        .iter_mut()
        .enumerate()
        .map(|(i, c)| {
            let id = c.id.clone();
            let metadata = c.metadata();

            let refspec = match &mut metadata.branch {
                // If the commit already had a branch, force push it
                Some(branch) => format!("+{}:{}", id, &branch),
                // If it didn't generate a new branch
                branch @ None => {
                    let branch_name = format!("refs/heads/fel/{}/{}", branch_name, i);
                    let refspec = format!("{}:{}", id, branch_name);
                    *branch = Some(branch_name);
                    refspec
                }
            };
            tracing::debug!(refspec, ?id, "got refspec");
            refspec
        })
        .collect();

    let mut callbacks = RemoteCallbacks::default();
    callbacks
        .sideband_progress(|message| {
            tracing::trace!(message = ?std::str::from_utf8(&message), "sideband progress");
            true
        })
        .push_transfer_progress(|a, b, c| {
            tracing::trace!(a, b, c, "transfer progress");
        })
        .push_negotiation(|updates| {
            let updates: Vec<_> = updates
                .iter()
                .map(|update| (update.src_refname(), update.dst_refname()))
                .collect();
            tracing::trace!(?updates, "negotiation");
            Ok(())
        })
        .push_update_reference(|branch, status| {
            tracing::trace!(branch, status, "update reference");
            Ok(())
        });

    tracing::debug!("pushing commits");
    let mut remote = repo.find_remote("origin").context("failed to get remote")?;
    let mut conn = remote
        .connect_auth(git2::Direction::Push, Some(auth::callbacks()), None)
        .context("failed to connect to remote")?;
    let remote = conn.remote();
    remote
        .push(
            &refspecs,
            Some(
                PushOptions::default()
                    .remote_callbacks(callbacks)
                    .follow_redirects(git2::RemoteRedirect::All),
            ),
        )
        .context("failed to push")?;
    tracing::debug!("push finished");

    // TODO Create github pullrequests

    tracing::debug!("Writing infos");
    for commit in commits.iter() {
        commit
            .flush_metadata(&repo)
            .context("failed to flush metadata")?;
    }

    Ok(())
}

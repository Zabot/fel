use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use git2::Repository;

mod auth;
mod config;
mod gh;
mod metadata;
mod push;
mod stack;
mod submit;
mod update;

use config::Config;
use stack::Stack;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short = 'C', value_name = "path", default_value = ".")]
    path: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Submit,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = Config::load().context("failed to load config")?;
    tracing_subscriber::fmt::init();

    // Make sure that notes.rewriteRef contains the namespace for fel notes so
    // they are copied along with commits during a rebase or ammend
    {
        let config = git2::Config::open_default().context("failed to open config")?;
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
    }

    let repo = Repository::discover(&cli.path).context("failed to open repo")?;

    // Push every commit
    let octocrab = Arc::new(
        octocrab::OctocrabBuilder::default()
            .personal_token(config.token.clone())
            .build()?,
    );

    let mut remote = repo
        .find_remote(&config.default_remote)
        .context("failed to get remote")?;

    let gh_repo = gh::get_repo(&remote).context("failed to get repo")?;
    let stack = Stack::new(
        &repo,
        &config,
    )
    .context("failed to get stack")?;

    submit::submit(&stack, &mut remote, &gh_repo, octocrab.clone(), &repo, &config)
        .await
        .context("failed to submit")?;

    Ok(())
}

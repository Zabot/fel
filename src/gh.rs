//use eyre::WrapErr;
use anyhow::{Context, Result};
use git2::Remote;
use git_url_parse::GitUrl;

pub struct GHRepo {
    pub owner: String,
    pub repo: String,
}

pub fn get_repo(remote: &Remote) -> Result<GHRepo> {
    let url = remote.url().context("failed to get remote url")?;
    let url = GitUrl::parse(url).unwrap(); //.context("failed to parse remote url")?;

    Ok(GHRepo {
        owner: url.owner.context("missing owner")?,
        repo: url.name,
    })
}

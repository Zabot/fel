use std::collections::HashMap;

use anyhow::{Context, Result};
use git2::Oid;
use parking_lot::RwLock;
use tera::Tera;
use tokio::sync::Notify;

use crate::stack::Stack;

pub trait StackRenderer {
    fn render(&self, commit: Oid, info: &[RenderInfo], stack: &Stack) -> Result<String>;
}

#[derive(serde::Serialize, Clone)]
pub struct RenderInfo {
    pub number: u64,
    pub title: String,
    pub commit: String,
}

pub struct RenderStore<R: StackRenderer> {
    commit_added: Notify,
    commit_info: RwLock<HashMap<Oid, RenderInfo>>,
    renderer: R,
}

impl<R: StackRenderer> RenderStore<R> {
    pub fn new(renderer: R) -> Self {
        Self {
            renderer,
            commit_added: Notify::new(),
            commit_info: RwLock::new(HashMap::new()),
        }
    }

    /// Record the render `info` for `commit`
    pub fn record(&self, commit: Oid, info: RenderInfo) {
        self.commit_info.write().insert(commit, info);
        self.commit_added.notify_waiters();
    }

    /// Get the render info for a specific commit
    async fn get_commit(&self, commit: &Oid) -> RenderInfo {
        loop {
            if let Some(info) = self.commit_info.read().get(commit) {
                return info.clone();
            }
            self.commit_added.notified().await
        }
    }

    /// Given a list of `commits`, wait until information is available for all
    /// of them, then return the info in the same order
    async fn get_info(&self, commits: &[Oid]) -> Vec<RenderInfo> {
        let mut info_vec = Vec::new();
        for commit in commits {
            let info = self.get_commit(commit).await;
            info_vec.push(info);
        }
        info_vec
    }

    pub async fn render_stack(&self, commit: Oid, stack: &Stack) -> Result<String> {
        let commits: Vec<_> = stack.iter().map(|c| c.id()).collect();
        let info = self.get_info(&commits).await;
        self.renderer.render(commit, &info, stack)
    }
}

// TODO This is totally overkill
pub struct TeraRender {
    tera: Tera,
}

impl TeraRender {
    pub fn new() -> Result<Self> {
        let mut tera = Tera::default();
        tera.add_raw_template("footer.html", include_str!("../templates/footer.html"))?;

        Ok(Self { tera })
    }
}

impl StackRenderer for TeraRender {
    fn render(&self, commit: Oid, info: &[RenderInfo], stack: &Stack) -> Result<String> {
        let mut context = tera::Context::new();
        context.insert("prs", info);
        context.insert("stack_name", stack.name());
        context.insert("upstream", stack.upstream());
        context.insert("current", &commit.to_string());

        let footer = self
            .tera
            .render("footer.html", &context)
            .context("render footer")?;
        tracing::debug!(footer, "rendered footer");
        Ok(footer)
    }
}

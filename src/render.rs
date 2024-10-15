use anyhow::{Context, Result};
use git2::Oid;
use tera::Tera;

use crate::{await_map::AwaitMap, stack::Stack};

pub trait StackRenderer {
    type RenderInfo: serde::Serialize + Clone;

    fn render(&self, commit: Oid, info: &[Self::RenderInfo], stack: &Stack) -> Result<String>;
}

pub struct RenderStore<R: StackRenderer> {
    commits: AwaitMap<Oid, R::RenderInfo>,
    renderer: R,
}

impl<R> RenderStore<R>
where
    R: StackRenderer,
{
    pub fn new(renderer: R) -> Self {
        Self {
            renderer,
            commits: AwaitMap::new(),
        }
    }

    /// Record the render `info` for `commit`
    pub fn record(&self, commit: Oid, info: R::RenderInfo) {
        self.commits.insert(commit, info)
    }

    /// Get the render info for a specific commit
    async fn get_commit(&self, commit: &Oid) -> R::RenderInfo {
        self.commits.get(commit).await
    }

    /// Given a list of `commits`, wait until information is available for all
    /// of them, then return the info in the same order
    async fn get_info(&self, commits: &[Oid]) -> Vec<R::RenderInfo> {
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

#[derive(serde::Serialize, Clone)]
pub struct TeraRenderInfo {
    pub number: u64,
    pub title: String,
    pub commit: String,
}

impl StackRenderer for TeraRender {
    type RenderInfo = TeraRenderInfo;

    fn render(&self, commit: Oid, info: &[Self::RenderInfo], stack: &Stack) -> Result<String> {
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

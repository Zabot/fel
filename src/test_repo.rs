use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use git2::{Oid, Remote, Repository, Signature, Time};

pub struct TestRepo {
    root: PathBuf,
    origin_repo: Repository,
    local_repo: Repository,
    initial_commit: Oid,
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

impl TestRepo {
    pub fn new() -> Self {
        let name = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();

        let test_root = env::temp_dir().join(name);

        // We explicitly create the dir so we know for sure it didn't exist before we started
        fs::create_dir(&test_root).unwrap();

        let origin_path = test_root.join("remote");
        let origin_repo = Repository::init_bare(&origin_path).expect("failed to create git repo");

        let local_path = test_root.join("local");
        let local_repo = Repository::clone(origin_path.to_str().unwrap(), local_path)
            .expect("failed to clone repo");

        let mut repo = Self {
            root: test_root.to_path_buf(),
            origin_repo,
            local_repo,
            initial_commit: Oid::zero(),
        };
        repo.initial_commit = repo.commit("initial");
        repo
    }

    pub fn initial_commit(&self) -> Oid {
        self.initial_commit
    }

    /// Create a new commit with the given commit message. The commit will update the
    /// contents of `initial` in the root of the repo
    pub fn commit(&self, msg: &str) -> Oid {
        let root = self.local_repo.workdir().expect("failed to get workdir");
        let contents = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();

        let relative_path = Path::new("initial");

        let parent: Vec<_> = self
            .local_repo
            .head()
            .iter()
            .map(|head| head.peel_to_commit().expect("failed to peel head"))
            .collect();
        let parent_ref: Vec<_> = parent.iter().collect();

        fs::write(root.join(relative_path), contents).expect("failed to write into repo");

        let sig = Signature::new("test", "test@test.test", &Time::new(0, 0))
            .expect("failed to create sig");

        let mut index = self.local_repo.index().expect("failed to get index");
        index
            .add_path(relative_path)
            .expect("failed to add to index");
        index.write().expect("failed to write index");

        let tree_id = index.write_tree().expect("failed to write tree");
        let tree = self
            .local_repo
            .find_tree(tree_id)
            .expect("failed to get tree");

        self.local_repo
            .commit(Some("HEAD"), &sig, &sig, msg, &tree, parent_ref.as_slice())
            .expect("failed to commit")
    }

    /// Change the head to the given `commit`
    pub fn checkout(&self, commit: Oid) {
        self.local_repo
            .head()
            .unwrap()
            .set_target(commit, "checkout")
            .unwrap();
    }

    /// Get a remote connection to the origin repo
    pub fn remote(&self) -> Remote {
        let mut remote_conn = self.local_repo.find_remote("origin").unwrap();
        remote_conn.connect(git2::Direction::Push).unwrap();
        remote_conn
    }

    /// Assert that the named `branch` is pointing to `commit` in the remote repo
    pub fn assert_pushed(&self, branch: &str, commit: Oid) {
        assert_eq!(
            self.origin_repo
                .refname_to_id(&format!("refs/heads/{branch}"))
                .unwrap(),
            commit
        );
    }
}

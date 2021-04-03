import pytest

from git import Repo

# Utility function that autogenerates a commit
def commit(repo, contents=None):
    if contents is None:
        try:
            prev = repo.head.commit.summary
            contents = str(int(prev) + 1)
        except ValueError:
            contents = '0'

    return repo.index.commit(contents)

@pytest.fixture
def upstream(tmpdir_factory):
    upstream = Repo.init(tmpdir_factory.mktemp("upstream"))
    commit(upstream)
    commit(upstream)
    commit(upstream)
    commit(upstream)
    return upstream

@pytest.fixture
def origin(upstream, tmpdir_factory):
    return upstream.clone(tmpdir_factory.mktemp("origin"))


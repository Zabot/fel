import pytest

from git import Repo

@pytest.fixture

@pytest.fixture
def repo(tmpdir_factory, commit):
    return Repo.init(tmpdir_factory.mktemp("repo"))

@pytest.fixture
def clone(repo, tmpdir_factory):
    return repo.clone(tmpdir_factory.mktemp("cloned"))

# Produces a complex branching repo
# * 078d4b8 (branch1) 4
# * 9c96605 3
# | * 752d474 (branch2) 12
# | * 60b94a4 11
# | | * 3d83fed (branch3) 10
# | | * 6d95ca2 9
# | |/
# | * 8d4d6b2 8
# | * f2f9e5a 7
# | | * 5ef83b3 (HEAD -> master) 14
# | | * f7eea99 13
# | |/
# | * 3559552 6
# | * e864436 5
# |/
# * 6051626 2
# * 4f7eeec 1
# * 45b78f0 0
@pytest.fixture
def branching_repo(repo, commit):
    master = repo.head.ref
    commit(repo)
    commit(repo)
    commit(repo)

    branch1 = repo.create_head("branch1")

    branch1.checkout()
    commit(repo)
    commit(repo)

    master.checkout()
    commit(repo)
    commit(repo)

    branch2 = repo.create_head("branch2")

    branch2.checkout()
    commit(repo)
    commit(repo)

    branch3 = repo.create_head("branch3")

    branch3.checkout()
    commit(repo)
    commit(repo)

    branch2.checkout()
    commit(repo)
    commit(repo)

    master.checkout()
    commit(repo)
    commit(repo)

    return repo

@pytest.fixture
def commit():
    class CommitFactory:
        commit_number = 0

        def _do_commit(self, repo):
            c = repo.index.commit(str(self.commit_number))
            self.commit_number += 1
            return c

    return CommitFactory()._do_commit


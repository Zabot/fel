import pytest

from git import Repo
from unittest.mock import Mock, MagicMock
from github.GithubException import GithubException

@pytest.fixture
def pr_factory():
    def _make(number):
        pr = Mock()

        pr.number = number
        pr.get_commits().totalCount = 1
        pr.get_commits().__getitem__ = Mock()
        pr.get_commits().__getitem__().get_check_runs.return_value=[]
        pr.get_commits()[0].get_combined_status().statuses = []
        pr.mergeable = True
        pr.mergeable_state = "clean"

        pr.get_reviews.return_value = []
        pr.base.repo.get_branch().get_protection.side_effect = GithubException(404, '{"message": "Branch not protected", "documentation_url": "https://docs.github.com/rest/reference/repos#get-branch-protection"}', None)

        return pr

    return _make

@pytest.fixture
def pr(pr_factory):
    return pr_factory(1)

@pytest.fixture
def ppr(pr_factory):
    pr = pr_factory(1)

    pr.base.repo.get_branch().get_protection.side_effect = None
    pr.base.repo.get_branch().get_protection.return_value = Mock()
    pr.base.repo.get_branch().get_protection().required_status_checks = None
    pr.base.repo.get_branch().get_protection().required_pull_request_reviews = None

    return pr

@pytest.fixture
def gh(branching_repo, pr_factory):
    mock = MagicMock()

    mock.pulls = [pr_factory(1)]

    def create_pull(**kwargs):
        pr = pr_factory(len(mock.pulls) + 1)
        pr.head.ref = kwargs['head']
        pr.base.ref = 'master'

        def merge(**kwargs):
            branching_repo.heads[pr.base.ref].checkout()
            branching_repo.git.cherry_pick(branching_repo.heads[pr.head.ref].commit)

            merge = branching_repo.heads[pr.base.ref].commit
            merge = merge.replace(message="{} ".format(merge.message))
            branching_repo.heads[pr.base.ref].set_commit(merge)

            branching_repo.heads['master'].checkout()

            return Mock(merged=True)

        pr.merge.side_effect = merge

        mock.pulls.insert(0, pr)
        return pr

    def get_pulls(**kwargs):
        return mock.pulls

    def get_pull(pr_num):
        return mock.pulls[-pr_num]

    mock.get_pulls.side_effect = get_pulls
    mock.get_pull.side_effect = get_pull
    mock.create_pull.side_effect = create_pull
    return mock

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
            f = open("{}/{}".format(repo.working_tree_dir, self.commit_number), 'a')
            f.close()

            repo.index.add([str(self.commit_number)])
            c = repo.index.commit(str(self.commit_number))

            self.commit_number += 1
            return c

    return CommitFactory()._do_commit


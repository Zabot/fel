import pytest

import fel.rebase

def index_map(repo):
    commits = {}
    for head in repo.heads:
        for commit in repo.iter_commits(head):
            commits[int(commit.summary)] = commit

    return commits

# Do a bunch of rebases to linearize master history
def test_linear_rebase(branching_repo):
    # Assert that every only the commits in should_rebase were rebased, and
    # that they were all rebased to the correct commit
    def assert_rebase(*should_rebase):
        should_rebase_commits = set([commits[i] for i in should_rebase])
        for c in should_rebase_commits:
            assert rebased[c].summary == c.summary
        assert should_rebase_commits == rebased.keys()

    orig_head = branching_repo.head.ref

    commits = index_map(branching_repo)
    rebased = fel.rebase.subtree_graft(branching_repo, commits[11], commits[10])
    assert_rebase(11, 12)

    commits = index_map(branching_repo)
    rebased = fel.rebase.subtree_graft(branching_repo, commits[13], commits[12])
    assert_rebase(13, 14)

    commits = index_map(branching_repo)
    rebased = fel.rebase.subtree_graft(branching_repo, commits[5], commits[4])
    assert_rebase(5, 6, 7, 8, 9, 10, 11, 12, 13, 14)

    # Asser that the history of master is an in order sequence
    path = [int(c.summary) for c in branching_repo.iter_commits(branching_repo.heads['master'])]
    assert path == list(reversed(range(0, 15)))

    assert branching_repo.head.ref == orig_head

# Rebase an entire subtree
def test_tree_rebase(branching_repo):
    def assert_rebase(*should_rebase):
        should_rebase_commits = set([commits[i] for i in should_rebase])
        for c in should_rebase_commits:
            assert rebased[c].summary == c.summary
        assert should_rebase_commits == rebased.keys()

    def assert_branch(branch, *commits):
        assert list(commits) == [int(c.summary) for c in branching_repo.iter_commits(branch)]

    orig_head = branching_repo.head.ref

    commits = index_map(branching_repo)
    rebased = fel.rebase.subtree_graft(branching_repo, commits[7], commits[14])

    assert_rebase(7, 8, 9, 10, 11, 12)
    assert_branch('branch1', 4, 3, 2, 1, 0)
    assert_branch('branch2', 12, 11, 8, 7, 14, 13, 6, 5, 2, 1, 0)
    assert_branch('branch3', 10, 9, 8, 7, 14, 13, 6, 5, 2, 1, 0)
    assert_branch('master', 14, 13, 6, 5, 2, 1, 0)

    assert branching_repo.head.ref == orig_head

# TODO Test rebases that squash commits

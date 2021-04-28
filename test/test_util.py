import pytest

import fel.util

def index_map(repo):
    commits = {}
    for head in repo.heads:
        for commit in repo.iter_commits(head):
            commits[int(commit.summary)] = commit

    return commits

def test_ancestry_path(branching_repo):
    commits = index_map(branching_repo)

    # Direct ancestor
    p = fel.util.ancestry_path(commits[1], commits[2])
    assert p == [commits[i] for i in [1, 2]]

    # Path goes through commits on current branch
    p = fel.util.ancestry_path(commits[1], commits[5])
    assert p == [commits[i] for i in [1, 2, 5]]

    # Path goes through commits not on current branch
    p = fel.util.ancestry_path(commits[1], commits[3])
    assert p == [commits[i] for i in [1, 2, 3]]

    # Path goes all the way to root
    p = fel.util.ancestry_path(commits[0], commits[3])
    assert p == [commits[i] for i in [0, 1, 2, 3]]

    # Path to self
    p = fel.util.ancestry_path(commits[3], commits[3])
    assert p == [commits[i] for i in [3]]

    # Child is younger then ancestor
    with pytest.raises(ValueError):
        p = fel.util.ancestry_path(commits[3], commits[0])

    # Commits not related
    with pytest.raises(ValueError):
        p = fel.util.ancestry_path(commits[12], commits[14])

def test_unique(branching_repo):
    _commits = index_map(branching_repo)

    # Upstream is active, ref is another branch
    c, mb = fel.util.get_first_unique(branching_repo, _commits[12], _commits[14])
    assert c == _commits[7]
    assert mb == _commits[6]

    # Upstream and ref are both inactive branches
    c, mb = fel.util.get_first_unique(branching_repo, _commits[12], _commits[10])
    assert c == _commits[11]
    assert mb == _commits[8]

    # TODO These cases probably should error
    # Upstream and ref are the same
    with pytest.raises(IndexError):
        c, mb = fel.util.get_first_unique(branching_repo, _commits[14], _commits[14])

    # Upstream and ref are related
    with pytest.raises(IndexError):
        c, mb = fel.util.get_first_unique(branching_repo, _commits[6], _commits[14])

# TODO The current behavior of subtree does not include the root commit in the
#      tree. This may not be the intended behavior.
def test_subtree(branching_repo):
    _commits = index_map(branching_repo)

    # Tip of active branch
    commits, branches = fel.util.get_subtree(branching_repo, _commits[14])
    assert commits == set([ _commits[i] for i in [] ])
    assert branches == [ branching_repo.refs[b] for b in ['master'] ]

    # Single child on active branch
    commits, branches = fel.util.get_subtree(branching_repo, _commits[13])
    assert commits == set([ _commits[i] for i in [14] ])
    assert branches == [ branching_repo.refs[b] for b in ['master'] ]

    # Tip of inactive branch
    commits, branches = fel.util.get_subtree(branching_repo, _commits[12])
    assert commits == set([ _commits[i] for i in [] ])
    assert branches == [ branching_repo.refs[b] for b in ['branch2'] ]

    # single child on inactive branch
    commits, branches = fel.util.get_subtree(branching_repo, _commits[11])
    assert commits == set([ _commits[i] for i in [12] ])
    assert branches == [ branching_repo.refs[b] for b in ['branch2'] ]

    # Mergebase of two branches
    commits, branches = fel.util.get_subtree(branching_repo, _commits[8])
    assert commits == set([ _commits[i] for i in [9, 10, 11, 12] ])
    assert branches == [ branching_repo.refs[b] for b in ['branch2', 'branch3'] ]

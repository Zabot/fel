from fel.submit import submit
from fel.land import land

# TODO Breaks when no existing PRS
def test_submit(branching_repo, clone, gh):
    clone.remotes[0].refs['branch1'].checkout()
    head = clone.create_head('branch1')
    head.checkout()
    submit(clone, head.commit, gh, clone.refs['master'], 'fel')

    assert gh.get_pulls.call_count == 2
    assert gh.create_pull.call_count == 2

    assert branching_repo.heads['fel/2'].commit.summary == '3'
    assert branching_repo.heads['fel/3'].commit.summary == '4'

# TODO Breaks when no existing PRS
def test_land(branching_repo, clone, gh):
    def assert_branch(r, branch, *commits):
        assert list(commits) == [int(c.summary) for c in r.iter_commits(branch)]

    # Do a submit to prepare for submission
    test_submit(branching_repo, clone, gh)

    head = clone.refs['branch1']
    land(clone, head.commit, gh, clone.refs['master'], 'fel')

    # Make sure every PR was merged once
    for pr in gh.pulls[:-1]:
        assert pr.merge.call_count == 1

    # Make sure master looks right in both repos
    assert_branch(branching_repo, 'master', 4, 3, 14, 13, 6, 5, 2, 1, 0)
    assert_branch(clone, 'master', 4, 3, 14, 13, 6, 5, 2, 1, 0)

# TODO Breaks when no existing PRS
def test_big_submit(branching_repo, clone, gh):
    clone.remotes[0].refs['branch2'].checkout()
    head = clone.create_head('branch2')
    head.checkout()
    submit(clone, head.commit, gh, clone.refs['master'], 'fel')

    assert gh.get_pulls.call_count == 4
    assert gh.create_pull.call_count == 4

    assert branching_repo.heads['fel/2'].commit.summary == '7'
    assert branching_repo.heads['fel/3'].commit.summary == '8'
    assert branching_repo.heads['fel/4'].commit.summary == '11'
    assert branching_repo.heads['fel/5'].commit.summary == '12'

# TODO Breaks when no existing PRS
def test_big_land(branching_repo, clone, gh):
    def assert_branch(r, branch, *commits):
        assert list(commits) == [int(c.summary) for c in r.iter_commits(branch)]

    # Do a submit to prepare for submission
    test_big_submit(branching_repo, clone, gh)

    head = clone.refs['branch2']
    land(clone, head.commit, gh, clone.refs['master'], 'fel')

    # Make sure every PR was merged once
    for pr in gh.pulls[:-1]:
        assert pr.merge.call_count == 1

    # Make sure master looks right in both repos
    assert_branch(branching_repo, 'master', 12, 11, 8, 7, 14, 13, 6, 5, 2, 1, 0)
    assert_branch(clone, 'master', 12, 11, 8, 7, 14, 13, 6, 5, 2, 1, 0)

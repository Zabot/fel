def assert_branching_repo(repo, prefix=''):
    assert [4, 3, 2, 1, 0] == [int(c.summary) for c in repo.iter_commits(prefix + 'branch1')]
    assert [12, 11, 8, 7, 6, 5, 2, 1, 0] == [int(c.summary) for c in repo.iter_commits(prefix + 'branch2')]
    assert [10, 9, 8, 7, 6, 5, 2, 1, 0] == [int(c.summary) for c in repo.iter_commits(prefix + 'branch3')]
    assert [14, 13, 6, 5, 2, 1, 0] == [int(c.summary) for c in repo.iter_commits(prefix + 'master')]

# These tests are both sanity checks to make sure the test fixtures are working
# as expected and don't test any logic
def test_branching_repo(branching_repo):
    assert_branching_repo(branching_repo)

def test_clone(branching_repo, clone):
    assert branching_repo != clone

    assert_branching_repo(branching_repo)
    assert_branching_repo(clone, 'origin/')


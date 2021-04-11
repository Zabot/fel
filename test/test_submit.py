from fel import submit

# TODO Breaks when no existing PRS
def test_submit(branching_repo, clone, gh):
    clone.remotes[0].refs['branch1'].checkout()
    head = clone.create_head('branch1')
    submit(clone, head.commit, gh, clone.refs['master'], 'fel')

    assert gh.get_pulls.call_count == 2
    assert gh.create_pull.call_count == 2

    assert branching_repo.heads['fel/2'].commit.summary == '3'
    assert branching_repo.heads['fel/3'].commit.summary == '4'

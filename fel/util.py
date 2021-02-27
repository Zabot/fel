# Get the list of commits from oldest to newest between ancestor and child
def ancestry_path(ancestor, child):
    lineage = [child]
    while ancestor != child:
        assert len(child.parents) == 1
        child = child.parents[0]

        lineage.append(child)

    return list(reversed(lineage))


# Find the oldest commit on branch that isn't on upstream in repo
def get_first_unique(repo, branch, upstream):
    mergebase = repo.merge_base(branch, upstream)
    assert len(mergebase) == 1
    lineage = ancestry_path(mergebase[0], branch)

    return lineage[1], mergebase[0]


# Find all of the commits and branches descendant from root in repo
def get_subtree(repo, root):
    # Get all of the branches that contain the root commit
    heads = [ head for head in repo.heads if repo.is_ancestor(root, head.commit) ]

    # Add all of the commits to the set
    commits = set()
    for head in heads:
        for c in repo.iter_commits("{}...{}".format(head, root)):
            commits.add(c)

    return commits, heads

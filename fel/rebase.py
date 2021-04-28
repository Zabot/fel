import logging

from .util import ancestry_path, get_subtree

# Subtree graft is broken when commits get squashed
# Rebase an entire subtree rooted at mergebase onto another commit
def subtree_graft(repo, root, onto, skip_root=False):
    logging.info("rebasing %s onto %s", root, onto)
    initial_head = repo.head.ref

    # Get all of the branches that contain the root commit
    _, heads = get_subtree(repo, root)

    # We can't graft a tree rooted at a merge commit
    if not skip_root:
        assert len(root.parents) == 1
        root_parent = root.parents[0]
    else:
        root_parent = root

    rebased_commits = {root_parent: onto}
    for head in heads:
        path = ancestry_path(root_parent, head.commit)
        assert path[0] == root_parent

        # Find most recent commit that has been rebased
        recent = root_parent
        for commit in path:
            if commit in rebased_commits:
                recent = commit

        # Rebase the part of this branch that hasn't been rebased yet onto its
        # parent in the rebased tree
        repo.git.rebase("--onto", rebased_commits[recent], recent, head.name)

        # Determine the new commits
        rebased_path = ancestry_path(onto, head.commit)
        logging.debug("%s rebased to %s", path, rebased_path)
        assert len(rebased_path) == len(path)

        for old, new in zip(path, rebased_path):
            # assert old.tree == new.tree
            rebased_commits[old] = new

    # We didn't actually rebase the root parent
    del rebased_commits[root_parent]

    initial_head.checkout()
    return rebased_commits

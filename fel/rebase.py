from github import Github
from git import Repo, Commit
import re
import logging

from .util import ancestry_path



# Like a traditional rebase, but also rebases any branches that have a mergebase
# with the rebased branch
def tree_rebase(repo, mergebase, upstream, onto):
    logging.info("rebasing %s onto %s", mergebase, onto)
    h = repo.head.commit

    # Get all of the branches that contain the root commit
    heads = [ head for head in repo.heads if repo.is_ancestor(mergebase, head.commit) ]

    rebased_commits = {mergebase: onto}
    for head in heads:
        path = ancestry_path(mergebase, head.commit)

        assert path[0] == mergebase

        # Find the newest commit that has been rebased
        oldest = mergebase
        for commit in path:
            if commit in rebased_commits:
                oldest = commit

        # Rebase the part of this branch that hasn't been rebased yet onto its
        # parent in the rebased tree
        old_head = head.commit
        output = repo.git.rebase("--onto", rebased_commits[oldest], oldest, head.name)

        # Determine the new commits
        rebased_path = ancestry_path(onto, head.commit)
        assert len(rebased_path) == len(path)

        for old, new in zip(path, rebased_path):
            # assert old.tree == new.tree
            rebased_commits[old] = new

    return rebased_commits


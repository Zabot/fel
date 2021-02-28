import logging

from .meta import parse_meta, dump_meta
from .rebase import tree_rebase
from .submit import submit

def land(repo, c, gh, upstream, branch_prefix):
    logging.info("landing %s on %s", c, upstream)

    # We can't handle merge commits
    assert len(c.parents) == 1

    # Don't land commits that are already upstream
    if repo.is_ancestor(c, upstream):
        logging.info("skipping upstream commit %s", c)
        return {}

    # Make sure that our parent is already landed
    rebased = land(repo, c.parents[0], gh, upstream, branch_prefix)

    # If landing c's parent rebased c, update c to what it was rebased to
    c = rebased.get(c, c)

    # Resubmit to update base branch in case we got rebased
    submit(repo, c, gh, upstream, branch_prefix)

    # Tell github to merge the PR
    message, meta = parse_meta(c.message)
    try:
        pr_num = meta['fel-pr']
        diff_branch = repo.heads[meta['fel-branch']]

        # Land the PR
        logging.info("merging %s", c)
        pr = gh.get_pull(pr_num)
        if not pr.mergeable:
            logging.error("Can't merge pr %s", pr.mergeable_state)

        status = pr.merge(merge_method='squash')
        if not status.merged:
            logging.error("Failed to merge pr %s", status.message)

        # Delete the branch
        # gh.get_branch(pr.head.ref).
        repo.delete_head(diff_branch)

        # Fetch the new commits
        repo.remote().fetch()

        # Get the remote ref of upstream
        remote_ref = repo.remote().refs[pr.base.ref]

        # rebase all children onto the pr base branch
        rebased_commits = tree_rebase(repo, c, c, remote_ref.commit)

        # Update the rebased commits (every key points to the final rebased
        # commit, skipping all the intermediate commits)
        rebased = {k: rebased_commits.get(v, v) for k, v in rebased.items()}
        rebased.update(rebased_commits)

    except KeyError:
        logging.error("Cant land unsubmitted commit")
        raise 

    return rebased

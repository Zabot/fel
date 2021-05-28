import logging

from github.GithubException import UnknownObjectException, GithubException

from .meta import parse_meta
from .rebase import subtree_graft
from .submit import submit
from .mergeability import is_mergeable, wait_for_checks

def land(repo, commit, gh_repo, upstream, branch_prefix, admin_merge=False, wait_for_merge=True):
    logging.info("landing %s on %s", commit, upstream)

    # We can't handle merge commits
    assert len(commit.parents) == 1

    # Don't land commits that are already upstream
    if repo.is_ancestor(commit, upstream):
        logging.info("skipping upstream commit %s", commit)
        return {}

    # Make sure that our parent is already landed
    rebased = land(repo, commit.parents[0], gh_repo, upstream, branch_prefix, admin_merge)

    # If landing commit's parent rebased commit, update commit to what it was rebased to
    commit = rebased.get(commit, commit)

    # Tell github to merge the PR
    _, meta = parse_meta(commit.message)
    try:
        pr_num = meta['fel-pr']
        diff_branch = repo.heads[meta['fel-branch']]

        # Land the PR
        logging.info("merging %s", commit)
        pr = gh_repo.get_pull(pr_num)


        mergeable, status, wait = is_mergeable(gh_repo, pr, upstream.name)

        if wait and wait_for_merge:
            mergeable, status = wait_for_checks(gh_repo, pr, upstream.name)

        if not mergeable:
            logging.error("Merge is blocked, run with --admin to force merge: %s", status)
            raise SystemExit()

        try:
            status = pr.merge(merge_method='squash')
            if not status.merged:
                logging.error("Failed to merge pr %s", status.message)
                raise SystemExit()

        except GithubException as ex:
            logging.error("Failed to merge pr %s", ex)
            raise SystemExit() from ex

        # Delete the branch
        # We can't delete the remote branch right away because that closes any
        # PRs stacked on top of this branch
        repo.delete_head(diff_branch)

        # Fetch the newly landed commit
        repo.remote().fetch()
        upstream.set_object(upstream.tracking_branch())

        # Get the remote ref of upstream
        remote_ref = repo.remote().refs[pr.base.ref]

        print("Landed PR #{} on {} as {}".format(pr_num, pr.base.ref, remote_ref.commit))

        # rebase all children onto the pr base branch
        rebased_commits = subtree_graft(repo, commit, remote_ref.commit, True)

        # Update the rebased commits (every key points to the final rebased
        # commit, skipping all the intermediate commits)
        rebased = {k: rebased_commits.get(v, v) for k, v in rebased.items()}
        rebased.update(rebased_commits)

        # Resubmit any commands that were rebased by this land
        for _, new in rebased_commits.items():
            try:
                submit(repo, new, gh_repo, upstream, branch_prefix, update_only=True)
            except ValueError:
                # If a commit hasn't been submitted yet, skip it
                pass

        try:
            # Delete the remote branch
            gh_repo.get_git_ref("heads/{}".format(pr.head.ref)).delete()
        except UnknownObjectException:
            logging.debug("Remote branch was already deleted")

    except KeyError:
        logging.error("Cant land unsubmitted commit")
        raise

    return rebased

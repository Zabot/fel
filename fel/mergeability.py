import time
from datetime import datetime, timedelta

from github.GithubException import GithubException
from yaspin import yaspin

# Returns two booleans. The first indicates if the PR is mergable, the second
# indicates if the lack of mergability is worth waiting for
#
# The github api doesn't actually give you the ability to determine why a PR
# is blocked from landing, and even if it did, fel branches don't play well
# with the branch protection rules. So we need to evaluate the branch
# protection rules ourself.
def is_mergeable(gh_repo, pr, upstream):
    # Before we do anything, check for conflicts
    if not pr.mergeable:
        return False, "Merge conflicts", False

    # Check for any reviews with requested changes
    changes_requested = 0
    approved = 0

    for review in pr.get_reviews():
        if review.state == 'CHANGES_REQUESTED':
            changes_requested += 1
        elif review.state == 'APPROVED':
            approved += 1

    # If there are any changes requested, we can't merge
    if changes_requested > 0:
        return False, "Changes requested", False

    # Get the branch protection configuration of the upstream branch
    # (This may not be the base of the PR, since the entire stack will eventually
    # be merged into the same branch, we check against the protection rules of
    # the final branch)
    upstream = pr.base.repo.get_branch(upstream)

    try:
        protection = upstream.get_protection()

        # Check for required number of approvals
        required_approvals = protection.required_pull_request_reviews
        if required_approvals != None:
            if approved < required_approvals.required_approving_review_count:
                return False, "Review required", False

        required_checks = set()

        if protection.required_status_checks:
            required_checks = set(protection.required_status_checks.contexts)

    except GithubException:
        # If there are no branch protection rules, no checks are required
        required_checks = set()

    # PR is ready to merge, lets make sure checks are passing
    commits = pr.get_commits()
    latest = commits[commits.totalCount - 1]

    # No conflicts, look for any pending checks
    pending = 0
    failed = 0
    total = 0
    for check in latest.get_check_runs():
        total += 1

        try:
            required_checks.remove(check.name)
        except KeyError:
            pass

        if check.status != 'completed':
            pending += 1
            continue

        if check.conclusion == 'failure':
            failed += 1
            continue

    # If the PR doesn't have the required checks run on it, we may not be close
    # enough to the upstream branch to trigger the checks to run.
    if required_checks:
        # If the PR was just updated, its possible that required checks haven't
        # been started yet. Give required checks a window of time to become
        # pending before failing for missing checks.
        if datetime.utcnow() - pr.updated_at < timedelta(seconds=10):
            return False, "Missing required checks", True

        return False, "Missing required checks", False

    if pending > 0:
        return False, "Waiting for checks ({} / {})".format(total - pending, total), True

    if failed > 0:
        return False, "Checks failed ({} / {})".format(total - failed, total), False

    # If we've checked everything, and the PR is still blocked, give up and notify
    pr = gh_repo.get_pull(pr.number)
    if pr.mergeable_state != 'clean':
        return False, "Unknown", False

    # There should be no way for an unmergeable PR to sneak all the way through
    # to this point
    assert pr.mergeable
    assert pr.mergeable_state == 'clean'

    return True, "", False

def wait_for_checks(gh_repo, pr, upstream, poll_interval=5):
    mergeable, status, wait = is_mergeable(gh_repo, pr, upstream)

    with yaspin(text="#{} {}".format(pr.number, status), color='yellow') as sp:
        while wait:
            sp.text = "#{} {}".format(pr.number, status)
            pr = gh_repo.get_pull(pr.number)
            mergeable, status, wait = is_mergeable(gh_repo, pr, upstream)

            if wait:
                time.sleep(poll_interval)

        sp.text = ''
        if mergeable:
            sp.color = 'green'
            sp.ok("✔ #{} Passing".format(pr.number))
            return True, ''
        else:
            sp.color = 'red'
            sp.fail("✖ #{} {}".format(pr.number, status))
            return False, status

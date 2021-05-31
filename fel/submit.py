import logging

from git import PushInfo

from .meta import parse_meta, dump_meta
from .rebase import subtree_graft


# This is a race condition because there is no way to create a PR without a
# branch. So we guess what the branch number should be, then try and create
# the PR ASAP so no one steals the PR number. If we get it wrong it doesn't
# matter because the commit is updated to have the actual PR afterwards
#
# Returns the ref that a commit stacked on top of this commit should base its PR
# on
def submit(repo, commit, gh_repo, upstream, branch_prefix, update_only=False):
    logging.info("submitting %s to %s", commit, upstream)

    # We can't handle merge commits
    assert len(commit.parents) == 1

    # Don't submit commits that are already upstream
    if repo.is_ancestor(commit, upstream):
        logging.info("skipping upstream commit %s", commit)
        return upstream, {}

    # Make sure that our parent is already submitted
    base_ref, rebased = submit(
            repo,
            commit.parents[0],
            gh_repo,
            upstream,
            branch_prefix,
            update_only
        )

    logging.info("pr base %s", base_ref)

    # If submitting commit's parent rebased commit, update commit to what it was rebased to
    commit = rebased.get(commit, commit)

    # Assert: At this point the parent commit's branch has been reset and force
    #         pushed to github

    # If the commit has been submitted before, grab the metadata and reset the
    # local head
    _, meta = parse_meta(commit.message)
    try:
        pr_num = meta['fel-pr']
        diff_branch = repo.heads[meta['fel-branch']]

        # Update the base branch (This can happen when a stack gets rebased
        # after the bottom gets landed). This causes churn, even if the result
        # is the same, so don't do it unless we need to
        pr = gh_repo.get_pull(pr_num)

        if pr.base != base_ref.tracking_branch().remote_head:
            pr.edit(base = base_ref.tracking_branch().remote_head)

        # Reset the local branch and push to github
        logging.info("updating PR %s", pr_num)
        diff_branch.set_commit(commit)
        push = repo.remote().push(diff_branch, force=True)
        if push[0].flags & PushInfo.UP_TO_DATE == 0:
            print("Updated PR #{} to {}".format(pr_num, commit))

    # If the commit hasn't been submitted before, create a new branch and PR
    # for it in the remote repo
    except KeyError as ex:
        if update_only:
            raise ValueError("Submitting unsubmitted commit with update_only = False") from ex

        print("Submitting PR for {}".format(commit))
        logging.info("creating a PR")

        # Guess GitHub PR number
        pr_num = gh_repo.get_pulls(state='all')[0].number + 1
        branch = "{}/{}".format(branch_prefix, pr_num)
        logging.info("creating branch %s for %s", branch, commit)
        diff_branch = repo.create_head(branch, commit=commit)

        # Create a remote branch and set diff_branch's tracking branch to it
        push_info = repo.remote().push(diff_branch)
        assert len(push_info) == 1
        diff_branch.set_tracking_branch(push_info[0].remote_ref)

        # Push branch to GitHub to create PR.
        try:
            summary, body = commit.message.split('\n', 1)
        except ValueError:
            summary = commit.message
            body = ""

        # If this repo has a pull request template, apply it to PR
        try:
            pr_template = repo.git.show('HEAD:.github/pull_request_template.md')
            if pr_template:
                body = body + '\n\n' + pr_template
        except:
            pass

        logging.info("creating pull for branch %s", branch)
        pr = gh_repo.create_pull(title=summary,
                            body=body,
                            head=diff_branch.tracking_branch().remote_head,
                            base=base_ref.tracking_branch().remote_head)

        # Update the metadata
        meta['fel-pr'] = pr.number
        meta['fel-branch'] = diff_branch.name

        # Amend the commit with fel metadata and push to GitHub
        amended = commit.replace(message = dump_meta(commit.summary, meta))
        diff_branch.set_commit(amended)
        repo.remote().push(diff_branch, force=True)

        # Restack the commits on top
        rebased_commits = subtree_graft(repo, commit, amended, skip_root=True)

        # Update the rebased commits (every key points to the final rebased
        # commit, skipping all the intermediate commits)
        rebased = {k: rebased_commits.get(v, v) for k, v in rebased.items()}
        rebased.update(rebased_commits)

    return diff_branch, rebased

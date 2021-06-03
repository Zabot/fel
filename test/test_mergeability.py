import pytest
import datetime

from unittest.mock import Mock

from fel.mergeability import is_mergeable

upstream = 'master'

@pytest.fixture
def gh_repo(pr):
    gh_repo = Mock()
    gh_repo.get_pull.return_value = pr
    return gh_repo

# Landing a pr on a branch with no protection rules and no conflicts
def test_clean_merge(gh_repo, pr):
    mergeable, status, wait = is_mergeable(gh_repo, pr, upstream)

    assert status == ''
    assert mergeable
    assert not wait

# Landing a pr on a branch with conflicts
def test_conflict_merge(gh_repo, pr):
    pr.mergeable = False

    mergeable, status, wait = is_mergeable(gh_repo, pr, upstream)

    assert status == 'Merge conflicts'
    assert not mergeable
    assert not wait

# Landing a pr when changes have been requested
def test_changes_merge(gh_repo, pr):
    pr.get_reviews.return_value = [Mock()]
    pr.get_reviews()[0].user.id = 1
    pr.get_reviews()[0].state = 'CHANGES_REQUESTED'

    mergeable, status, wait = is_mergeable(gh_repo, pr, upstream)

    assert status == 'Changes requested'
    assert not mergeable
    assert not wait

# Landing a pr with pending checks
def test_pending_checks(gh_repo, pr):
    pr.get_commits()[0].get_check_runs.return_value=[Mock()]
    pr.get_commits()[0].get_check_runs()[0].name = 'check'
    pr.get_commits()[0].get_check_runs()[0].status = 'pending'
    pr.get_commits()[0].get_check_runs()[0].conclusion = None

    mergeable, status, wait = is_mergeable(gh_repo, pr, upstream)

    assert status == 'Waiting for checks (0 / 1)'
    assert not mergeable
    assert wait

# Landing a pr with failed checks
def test_failed_checks(gh_repo, pr):
    pr.get_commits()[0].get_check_runs.return_value=[Mock()]
    pr.get_commits()[0].get_check_runs()[0].name = 'check'
    pr.get_commits()[0].get_check_runs()[0].status = 'completed'
    pr.get_commits()[0].get_check_runs()[0].conclusion = 'failure'

    mergeable, status, wait = is_mergeable(gh_repo, pr, upstream)

    assert status == 'Checks failed (0 / 1)'
    assert not mergeable
    assert not wait

# Landing a PR to a protected branch without reviews
def test_missing_reviews(gh_repo, ppr):
    ppr.get_reviews.return_value = []
    ppr.base.repo.get_branch().get_protection().required_pull_request_reviews = Mock()
    ppr.base.repo.get_branch().get_protection().required_pull_request_reviews.required_approving_review_count = 1

    mergeable, status, wait = is_mergeable(gh_repo, ppr, upstream)

    assert status == 'Review required'
    assert not mergeable
    assert not wait

# Landing a PR to a protected branch with reviews
def test_approved_reviews(gh_repo, ppr):
    ppr.get_reviews.return_value = [Mock()]
    ppr.get_reviews()[0].user.id = 1
    ppr.get_reviews()[0].state = 'APPROVED'
    ppr.base.repo.get_branch().get_protection().required_pull_request_reviews = Mock()
    ppr.base.repo.get_branch().get_protection().required_pull_request_reviews.required_approving_review_count = 1

    mergeable, status, wait = is_mergeable(gh_repo, ppr, upstream)

    assert status == ''
    assert  mergeable
    assert not wait

# Landing a pr with missing required status checks
def test_missing_status(gh_repo, ppr):
    ppr.updated_at = datetime.datetime(year=1970, month=1, day=1)
    ppr.base.repo.get_branch().get_protection().required_status_checks = Mock()
    ppr.base.repo.get_branch().get_protection().required_status_checks.contexts = ['check']

    mergeable, status, wait = is_mergeable(gh_repo, ppr, upstream)

    assert status == 'Missing required checks'
    assert not mergeable
    assert not wait

# Landing a new pr with missing required status checks
def test_missing_status_new_pr(gh_repo, ppr):
    ppr.updated_at = datetime.datetime.utcnow()
    ppr.base.repo.get_branch().get_protection().required_status_checks = Mock()
    ppr.base.repo.get_branch().get_protection().required_status_checks.contexts = ['check']

    mergeable, status, wait = is_mergeable(gh_repo, ppr, upstream)

    assert status == 'Missing required checks'
    assert not mergeable
    assert wait

# Landing a pr with passed required status checks
def test_passed_status(gh_repo, ppr):
    ppr.base.repo.get_branch().get_protection().required_status_checks = Mock()
    ppr.base.repo.get_branch().get_protection().required_status_checks.contexts = ['check']
    ppr.get_commits()[0].get_check_runs.return_value=[Mock()]
    ppr.get_commits()[0].get_check_runs()[0].name = 'check'
    ppr.get_commits()[0].get_check_runs()[0].status = 'completed'
    ppr.get_commits()[0].get_check_runs()[0].conclusion = 'success'

    mergeable, status, wait = is_mergeable(gh_repo, ppr, upstream)

    assert status == ''
    assert mergeable
    assert not wait

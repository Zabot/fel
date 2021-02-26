from github import Github
from git import Repo, Commit
import re

import logging
logging.basicConfig(level=logging.INFO)

from submit import submit
from land import land

ssh_re = re.compile("git@github.com:(.*/.*)\.git")
def parse_url(url):
    m = ssh_re.match(url)
    if m != None:
        return m.group(1)

top_ref = 'HEAD'
base_ref = 'master'

repo = Repo('.')
top = repo.commit(top_ref)


g = Github("23829d31b8af75fa76549837e0da627a06510722")
username = g.get_user().login.lower()
gh_slug  = parse_url(list(repo.remote().urls)[0])
gh_repo = g.get_repo(gh_slug)

bottom = repo.merge_base(top, "origin/master")
assert len(bottom) == 1
bottom = bottom[0]

assert repo.is_ancestor(bottom, top)

# submit(repo, top, gh_repo, repo.heads['master'], username)
land(repo, top, gh_repo, repo.remote().refs['master'], username, top)


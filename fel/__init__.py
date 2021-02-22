from github import Github
from git import Repo, Commit
import re

def adjacent_pairs(iterable):
    _iter = iter(iterable)
    prev = next(_iter)
    for e in _iter:
        yield (prev, e)
        prev = e


def ancestry_path(ancestor, child):
    lineage = [child]
    while ancestor != child:
        assert len(child.parents) == 1
        child = child.parents[0]

        lineage.append(child)

    return list(reversed(lineage))
        

ssh_re = re.compile("git@github.com:(.*/.*)\.git")
def parse_url(url):
    m = ssh_re.match(url)
    if m != None:
        return m.group(1)

# Like a traditional rebase, but also rebases any branches that have a mergebase
# with the rebased branch
def tree_rebase(repo, mergebase, upstream, onto):
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
            assert old.tree == new.tree
            rebased_commits[old] = new

    # repo.head = h
    return rebased_commits

top_ref = 'HEAD'
base_ref = 'master'

repo = Repo('.')
origin = repo.remote()
top = repo.commit(top_ref)


g = Github("23829d31b8af75fa76549837e0da627a06510722")
username = g.get_user().login.lower()
gh_slug  = parse_url(list(repo.remote().urls)[0])
gh_repo = g.get_repo(gh_slug)

def parse_meta(c):
    kvs = c.message.split("\n\n")[-1].strip().split('\n')
    meta = dict([kv.split(': ') for kv in kvs])

    return meta

# This is a race condition because there is no way to create a PR without a
# branch. So we guess what the branch number should be, then try and create
# the PR ASAP so no one steals the PR number. If we get it wrong it doesn't
# matter because the commit is updated to have the actual PR afterwards
def submit(c):
    #meta = parse_meta(c)
    #pr_num = meta['fel-pr']

    # Guess GitHub PR number
    pr_num = gh_repo.get_pulls(state='all')[0].number + 1
    diff_branch = repo.create_head("gh/{}/{}".format(username, pr_num), commit=c)

    assert len(c.parents) == 1
    try:
        parent_meta = parse_meta(c.parents[0])
    except ValueError:
        parent_meta = {'fel-branch': 'master'}

    # Push branch to GitHub to create PR
    origin.push(diff_branch)
    pr = gh_repo.create_pull(title=c.summary, body="", head=diff_branch.name, base=parent_meta['fel-branch'])

    # Amend the commit with fel metadata and push to GitHub
    amended = c.replace(message = "{}\n\nfel-pr: {}\nfel-branch: {}\n".format(c.summary, pr.url, diff_branch.name))
    diff_branch.set_commit(amended)
    origin.push(diff_branch, force=True)

    # Restack the commits on top
    rebased_commits = tree_rebase(repo, c, c, amended)

    return None, rebased_commits

bottom = repo.merge_base(top, base_ref)
assert len(bottom) == 1
bottom = bottom[0]

assert repo.is_ancestor(bottom, top)

rebased_commits = {}

def resolve(c):
    while True:
        try:
            c = rebased_commits[c]
        except KeyError:
            return c

for c in ancestry_path(bottom, top)[1:]:
    pr, rb = submit(resolve(c))
    rebased_commits.update(rb)


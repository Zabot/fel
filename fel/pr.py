import logging

from .meta import parse_meta
from .stack import render_stack

def update_prs(tree, gh_repo):
    commits = []
    lines = []
    for prefix, commit in tree:
        # If there is no commit for this line, print it without changes
        if commit is None:
            lines.append(prefix)
            continue

        # If there is a commit, get the PR from it
        try:
            _, meta = parse_meta(commit.message)
            pr_num = meta['fel-pr']

            lines.append("{prefix}<a href=\"{pr}\">#{pr} {summary}</a>"
                    .format(prefix = prefix,
                            pr = pr_num,
                            summary=commit.summary))

            commits.append(commit)

        except KeyError:
            # Skip commits that haven't been published
            logging.info("ignoring unpublished commit %s", commit)

    for commit in commits:
        if commit is None:
            continue

        _, meta = parse_meta(commit.message)
        pr_num = meta['fel-pr']
        pr = gh_repo.get_pull(pr_num)

        separator = '\n[#]:fel\n'
        try:
            block_start = pr.body.index(separator)
            body = pr.body[0: block_start]
        except ValueError:
            body = pr.body

        body = ("{original_body}\n"
                "{separator}\n"
                "---\n"
                "This diff is part of a [fel stack](https://github.com/zabot/fel)\n"
                "<pre>\n"
                "{tree}\n"
                "</pre>\n").format(
                        original_body=body,
                        separator=separator,
                        tree='\n'.join(lines))

        pr.edit(body = body)

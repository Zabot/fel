from .stack_spinner import ThreadGroup
from .meta import meta
from .stack import StackProgress
from . import style


def update_prs(gh_repo, stack, progress):
    with progress.start("Rewriting PRs"):
        with ThreadGroup() as tg:
            stack_string = StackProgress(stack, None, None)
            for commit in stack.commits():
                pr_num = meta(commit, "fel-pr")
                stack_string[
                    commit
                ] = f'<a href="{pr_num}">#{pr_num} {commit.summary}</a>'

            for commit in stack.commits():

                def update_pr(commit, pr_num):
                    try:
                        progress[
                            commit
                        ] = f"{style.context}#{pr_num} {style.info}{{spinner}} Rewriting PRs{style.default} {commit.summary}"

                        pr = gh_repo.get_pull(pr_num)

                        separator = "[#]:fel"
                        try:
                            block_start = pr.body.index(separator)
                            body = pr.body[0:block_start].strip()
                        except ValueError:
                            body = pr.body

                        new_body = (
                            f"{body}"
                            f"\n\n{separator}\n\n"
                            f"---\n"
                            f"This diff is part of a [fel stack](https://github.com/zabot/fel)\n"
                            f"<pre>\n"
                            f"{stack_string}\n"
                            f"</pre>\n"
                        )

                        # Github API uses DOS EOL, strip those so we can compare
                        if pr.body.replace("\r\n", "\n") != new_body:
                            pr.edit(body=new_body)
                            progress[
                                commit
                            ] = f"{style.context}#{pr_num} {style.warn}[updated body]{style.default} {commit.summary}"
                        else:
                            progress[
                                commit
                            ] = f"{style.context}#{pr_num} {style.ok}[up to date]{style.default} {commit.summary}"

                    except KeyError:
                        progress[
                            commit
                        ] = f"{style.context}{style.info}[skipped]{style.default} {commit.summary}"

                pr_num = meta(commit, "fel-pr")
                tg.do(update_pr, commit, pr_num)

from re import compile as re_compile

# Monkey patch print in the filter repo module so we can get silent output
import git_filter_repo as fr

from . import __version__
from .util import get_subtree, get_first_unique
from .meta import parse_meta, dump_meta, meta
from .style import ok, warn, fail, default, wrap, context, info

sha_re = re_compile("[0-9a-f]{40}")


def simple_info(commit, extra=""):
    _, meta = parse_meta(commit.message)
    try:
        return f"{context}#{meta['fel-pr']}{default}{extra} {commit.summary}"

    except KeyError:
        try:
            return f"{context}{meta['fel-branch']}{default}{extra} {commit.summary}"

        except KeyError:
            return f"{context}{commit.hexsha[:8]}{default}{extra} {commit.summary}"


class Stack:
    """
    Stack tracks the commits that are part of a stack in a local repo. The stack
    itself is independent of the remote repository.
    """

    def __init__(self, repo, branch, upstream, branch_prefix="fel"):
        self.repo = repo
        self.branch = repo.head
        self._stack = repo.head.ref
        self.upstream = upstream

        self.root, _ = get_first_unique(self.repo, self.branch.commit, self.upstream)

    def commits(self):
        """Return all of the commits in this stack."""

        # Find the first commit on branch that isn't in upstream
        root, _ = get_first_unique(self.repo, self.branch.commit, self.upstream)

        # Find all of the commits in the tree
        commits, refs = get_subtree(self.repo, root)

        # Find all of the commits between HEAD and mergebase
        commits.add(root)

        s = [c for c in commits]
        return s

    def filter(self, callback):
        """Run callback to rewrite every commit in the stack"""

        def commit_callback(commit, metadata):
            # Determine the commit encoding
            encoding = "utf-8"
            if commit.encoding is not None:
                encoding = commit.encoding

            # Decode the sha and message
            sha = commit.original_id.decode(encoding)
            m = commit.message.decode(encoding)

            # Decode the metadata
            message, meta = parse_meta(m)

            # Transform the metadata
            old_meta = meta.copy()
            callback(sha, commit, meta)

            # Amend the commit if the metadata changed
            if meta != old_meta:
                meta["fel-amended-from"] = sha
                commit.message = dump_meta(message, meta).encode(encoding)

        # Run filter over all commits that are accesible from HEAD, but not from upstream
        args = fr.FilteringOptions.parse_args(
            [
                "--refs",
                "HEAD",
                "^" + self.upstream.name,
                "--source",
                self.repo.working_dir,
                "--target",
                self.repo.working_dir,
                "--quiet",
                "--force",
            ]
        )
        filter = fr.RepoFilter(args, commit_callback=commit_callback)

        # Monkeypatch the builtin print in the git-filter-repo module to silence
        # output
        old_print = fr.__builtins__["print"]
        fr.__builtins__["print"] = lambda *args: None
        filter.run()
        fr.__builtins__["print"] = old_print

    def annotate(self, progress, branch_prefix="fel"):
        """
        Annotate every commit in the stack with the name and index of the
        stack.
        """
        # The name of the stack is the name of the currently checked out branch
        stack = self.repo.head.ref

        with progress.start("Annotating branches", False):

            # Keep track of the filter-repo id of each commit we find
            commit_indexes = {}

            def do_annotate(sha, commit, meta):
                if "fel-stack" not in meta:
                    # If this is the first commit in the stack, it has an index of 0
                    if sha == self.root.hexsha:
                        stack_index = 0

                    else:
                        # Get the parent commit
                        assert len(commit.parents) == 1
                        parent_stack, parent_index = commit_indexes[commit.parents[0]]

                        # If this commit is in the same stack as its parent, increase
                        # the id by 1, if not it must be in a new stack
                        if parent_stack == stack:
                            stack_index = parent_index + 1
                        else:
                            stack_index = 0

                    # Update the commit metadata
                    meta["fel-version"] = __version__
                    meta["fel-stack"] = stack
                    meta["fel-stack-index"] = stack_index
                    meta["fel-branch"] = "{}/{}/{}".format(
                        branch_prefix, meta["fel-stack"], meta["fel-stack-index"]
                    )
                    commit_indexes[commit.id] = (stack, stack_index)

            # Run do_annotate over all of the commits in this stack
            self.filter(do_annotate)

    # TODO Whitelist to only push certain commits
    def push(self, progress):
        """
        Create a branch for each commit in the stack and push them to remote.
        """
        with progress.start("Pushing branches"):
            stack_branches = []
            for c in self.commits():
                branch = meta(c, "fel-branch")
                stack_branches.append(self.repo.create_head(branch, c, force=True))

            # Create a remote branch and set diff_branch's tracking branch to it
            push_info = self.repo.remote().push(stack_branches, force=True)
            assert len(push_info) == len(stack_branches)

            for _info in push_info:
                _info.local_ref.set_tracking_branch(_info.remote_ref)

                commit = _info.local_ref.commit
                try:
                    pr = "#" + str(meta(commit, "fel-pr"))
                except KeyError:
                    pr = meta(commit, "fel-branch")

                info_summary = _info.summary.strip()

                if _info.flags & _info.UP_TO_DATE:
                    info_summary = wrap(info_summary, ok)
                elif _info.flags & (
                    _info.FORCED_UPDATE
                    | _info.FAST_FORWARD
                    | _info.NEW_HEAD
                    | _info.DELETED
                ):
                    info_summary = wrap(info_summary, warn)
                else:
                    info_summary = wrap(info_summary, fail)

                progress[commit] = "{}{}{} {} {}".format(
                    context, pr, default, info_summary, commit.summary
                )

    def render_stack(self, callback, color):
        # Use git log to print an ASCII graph of the tree using only full shas
        # so we can regex them later
        tree = self.repo.git.log(
            "--graph",
            "--pretty=format:%H",
            self.branch.commit,
            "^" + self.upstream.name,
        )

        # Expand each sha in the graph
        commits = []
        lines = []
        for line in tree.split("\n"):
            try:
                (sha,) = sha_re.findall(line)
                commit = self.repo.commit(sha)

                if commit:
                    summary = f"{{{commit.hexsha}}}"
                    if color is None:
                        lines.append(sha_re.sub(summary, line))
                    else:
                        lines.append(sha_re.sub(default + summary, color + line))
                    commits.append(commit)

            except ValueError:
                lines.append(line)

        if color is None:
            lines.append(f"* {self.upstream.name}")
        else:
            # Append the upstream branch name to put the stack in context
            lines.append(f"{color}* {self.upstream.name}{default}")

        return "\n".join(lines), commits


class StackProgress:
    def __init__(self, stack, write, color=context, verbose=True):
        self.status = None
        self.write = write
        self.commits = {}
        self.verbose = verbose
        self.hide_tree = False

        skeleton_tree, commits = stack.render_stack(lambda x: None, color)
        for c in commits:
            if c not in self.commits:
                self.commits[c] = simple_info(c)

        self.skeleton_tree = skeleton_tree

    def all(self, text):
        self.hide_tree = False
        for commit in self.commits.keys():
            self.commits[commit] = simple_info(commit, text)

    def __setitem__(self, commit, text):
        self.commits[commit] = text

    def __str__(self):
        tree = ""
        if not self.hide_tree:
            tree = self.skeleton_tree.format(
                **{c.hexsha: v for c, v in self.commits.items()}
            )

        if self.status is not None:
            tree = f"{tree}\n{info}{{spinner}}{default} {self.status}"

        return tree

    def ok(self):
        status = self.status
        self.status = None
        if not self.verbose or self.hide_tree:
            self.write(f"{ok}OK{default} {status}")
        else:
            self.write(f"{self}\n{ok}OK{default} {status}")

    def fail(self, reason):
        status = self.status
        self.status = None
        if self.verbose:
            self.write(f"{self}\n{fail}FAIL{default} {status} ({reason})")
        else:
            self.write(f"{fail}FAIL{default} {status} ({reason})")

    class Subtask:
        def __init__(self, parent):
            self.parent = parent

        def __enter__(self):
            pass

        def __exit__(self, exc_type, exc_value, exc_traceback):
            if exc_value is None:
                self.parent.ok()
            else:
                self.parent.fail(f"{exc_type}({exc_value})")

    def start(self, task, stack=True):
        self.status = task

        if stack:
            self.all(f" {info}{{spinner}} {task}{default}")
        else:
            self.hide_tree = not stack

        return self.Subtask(self)

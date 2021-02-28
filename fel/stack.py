from re import compile

from .util import get_subtree, get_first_unique
from .meta import parse_meta


sha_re = compile('[0-9a-f]{40}')
def render_stack(repo, branch, upstream):
    # Find the root of the tree
    root, mergebase = get_first_unique(repo, branch, upstream)

    # Find all of the commits in the tree
    _, refs = get_subtree(repo, root)

    # Use git log to print an ASCII graph of the tree using only full shas
    # so we can regex them later
    tree = repo.git.log("--graph",
                        "--pretty=format:%H",
                        *refs,
                        "^{}".format(root.parents[0].parents[0]))

    # Expand each sha in the graph
    lines = []
    for line in tree.split('\n'):
        try:
            sha, = sha_re.findall(line)
            c = repo.commit(sha)

            # Only show the mergebase as a ref
            summary = ''
            if c == mergebase:
                summary = upstream.name
                c = None

            lines.append((sha_re.sub(summary, line), c))

        except ValueError:
            lines.append((line, None))

    return lines

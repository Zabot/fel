import argparse
import logging
import re

from pathlib import Path

import git
import requests

from github import Github
from github.GithubException import UnknownObjectException

from . import __version__
from .config import load_config
from .submit import submit
from .land import land
from .stack import render_stack
from .pr import update_prs
from .meta import parse_meta
from .mergeability import is_mergeable

def _submit(repo, gh_repo, _, config):
    submit(repo,
           repo.head.commit,
           gh_repo,
           repo.heads[config['upstream']],
           config['branch_prefix'])

    tree = render_stack(repo,
                        repo.head.commit,
                        repo.heads[config['upstream']])

    update_prs(tree, gh_repo)

def _land(repo, gh_repo, args, config):
    land(repo,
         repo.head.commit,
         gh_repo,
         repo.heads[config['upstream']],
         config['branch_prefix'],
         admin_merge=args.admin,
         )

    repo.remote().fetch(prune=True)

def _status(repo, gh_repo, __, config):
    upstream = config['upstream']
    tree = render_stack(repo,
                        repo.head.commit,
                        repo.heads[upstream])

    for prefix, commit in tree:
        # If there is no commit for this line, print it without changes
        if commit is None:
            print("\033[33m{}\033[0m".format(prefix))
            continue

        # If there is a commit, get the PR from it
        try:
            _, meta = parse_meta(commit.message)
            pr_num = meta['fel-pr']

            pr = gh_repo.get_pull(pr_num)

            mergeable, message, temp = is_mergeable(gh_repo, pr, upstream)

            m = ""
            if mergeable:
                m = '\033[32m ✓'
            elif temp:
                m = '\033[33m • '
            else:
                m = '\033[31m ✖ '

            m += message + '\033[0m'

            print("\033[33m{}#{}\033[0m{} {}".format(prefix, pr_num, m, commit.summary))

        except KeyError:
            # Skip commits that haven't been published
            logging.info("ignoring unpublished commit %s", commit)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('-C',
                        metavar='path',
                        type=Path,
                        help='change directory to path before running fel',
                        )
    parser.add_argument('-f', '--config',
                        metavar='config',
                        type=Path,
                        help='fel config file',
                        default=Path.home().joinpath('.fel.yml'),
                        )
    parser.add_argument('--verbose',
                        action='store_true',
                        help='display verbose logging information',
                        )
    parser.add_argument('--version',
                        action='store_true',
                        help='display version information',
                        )

    subparsers = parser.add_subparsers()

    submit_parser = subparsers.add_parser('submit')
    submit_parser.set_defaults(func=_submit)

    land_parser = subparsers.add_parser('land')
    land_parser.add_argument('--admin',
                             action='store_true',
                             help='admin merge all PRs',
                             )
    land_parser.set_defaults(func=_land)

    status_parser = subparsers.add_parser('status')
    status_parser.set_defaults(func=_status)

    args = parser.parse_args()

    try:
        config = load_config(args.config)
    except IOError as ex:
        logging.error("Could not open config file: %s", ex)
        return 1
    except KeyError as ex:
        logging.error("Missing required config field: %s", ex)
        return 2

    if config['check_for_updates']:
        latest = requests.get('https://pypi.org/pypi/fel/json').json()['info']['version']
        if latest != __version__:
            print("You are running fel {}, the latest is {}".format(__version__, latest))

    if args.verbose:
        logging.basicConfig(level=logging.INFO)

    if args.version:
        print("fel {} from {}".format(__version__, __file__))
        return 0

    # Find the repo root
    if args.C:
        repo_root = args.C
    else:
        repo_root = git.Git().rev_parse("--show-toplevel")
    repo = git.Repo(repo_root)

    # Login to github and find the repo
    gh_client = Github(config['gh_token'])

    # Get the fel branch prefix
    username = gh_client.get_user().login.lower()
    config['branch_prefix'] = "fel/{}".format(username)

    # Find the github repo associated with the local repo's remote
    try:
        remote_url = next(repo.remote().urls)
        match = re.match(r"(?:git@|https://)github.com[:/](.*/.*)", remote_url)
        gh_slug = match.group(1)
        if gh_slug.endswith('.git'):
            gh_slug = gh_slug[:-4]
        gh_repo = gh_client.get_repo(gh_slug)

        # Run the sub command
        args.func(repo, gh_repo, args, config)

    except ValueError as ex:
        logging.error("Could not find remote repo: %s", ex)
        return 3

    except UnknownObjectException as ex:
        logging.error("Could not find remote repo on github: %s", ex)
        return 3

    return 0

if __name__ == '__main__':
    main()

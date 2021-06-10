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
from .submit import submit, submit_stack
from .land import land
from .stack import Stack, StackProgress
from .pr import update_prs
from .meta import parse_meta
from .mergeability import is_mergeable
from .style import *
from .stack_spinner import Spinner, ThreadGroup
import time

def _submit(repo, gh_repo, _, config):
    stack = Stack(repo, repo.head.commit, repo.heads[config['upstream']])
    with Spinner('') as spinner:
        sp = StackProgress(stack, spinner.print)
        spinner.label = sp

        # Update each commit with an index in the stack
        stack.annotate(sp)

        # Update and push all of the stack branches
        stack.push(sp)

        # Update the PR for each commit in the stack
        submit_stack(gh_repo, stack, sp)

        # Rewrite the PRs to include the fel stack
        update_prs(gh_repo, stack, sp)

def _land(repo, gh_repo, args, config):
    land(repo,
         repo.head.commit,
         gh_repo,
         repo.heads[config['upstream']],
         config['branch_prefix'],
         admin_merge=args.admin,
         )

    repo.remote().fetch(prune=True)

def _stack(repo, gh_repo, args, config):
    s = Stack(repo, repo.head.commit, repo.heads[config['upstream']])
    s.annotate()
    s.push()

def _status(repo, gh_repo, __, config):
    stack = Stack(repo, repo.head.commit, repo.heads[config['upstream']])

    with Spinner('') as spinner:
        sp = StackProgress(stack, spinner.print)
        spinner.label = sp

        with sp.start('Fetching PR Info'):
            def get_status(commit, pr_num):
                """Retrieve the status of a pull request"""
                pr = gh_repo.get_pull(pr_num)

                mergeable, message, temp = is_mergeable(gh_repo, pr, config['upstream'])

                icon = ""
                if mergeable:
                    icon = ok + '✓'
                elif temp:
                    icon = warn + '• '
                else:
                    icon = fail + '✖ '

                status = f"{icon}{message}{default}"

                sp[commit] = f"{context}#{pr_num}{default} {status} {commit.summary} {dull}{pr_link}{default}"

            with ThreadGroup() as tasks:
                for commit in stack.commits():
                    _, meta = parse_meta(commit.message)
                    try:
                        pr_num = meta['fel-pr']
                        pr_link = f"{gh_repo.html_url}/pull/{pr_num}"

                        tasks.do(get_status, commit, pr_num)
                        sp[commit] = f"{context}#{pr_num}{default} {info}{{spinner}} Fetching PR info{default} {commit.summary} {dull}{pr_link}{default}"

                    except KeyError:
                        try:
                            branch = meta['fel-branch']
                            sp[commit] = f"{context}{branch}{default} {commit.summary}"

                        except KeyError:
                            sp[commit] = f"{context}{commit.hexsha[:8]}{default} {commit.summary}"

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

    stack_parser = subparsers.add_parser('stack')
    stack_parser.set_defaults(func=_stack)

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

        # Run the sub command
        args.func(repo, None, args, config)

    except UnknownObjectException as ex:
        logging.error("Could not find remote repo on github: %s", ex)
        return 3

    return 0

if __name__ == '__main__':
    main()

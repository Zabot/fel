import argparse
import logging
import re

from pathlib import Path

import git
import yaml
import requests

from github import Github

from . import __version__
from .submit import submit
from .land import land
from .stack import render_stack
from .pr import update_prs
from .meta import parse_meta

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

def _land(repo, gh_repo, _, config):
    land(repo,
         repo.head.commit,
         gh_repo,
         repo.heads[config['upstream']],
         config['branch_prefix'])

    repo.remote().fetch(prune=True)

def _status(repo, _, __, config):
    tree = render_stack(repo,
                        repo.head.commit,
                        repo.heads[config['upstream']])

    for prefix, commit in tree:
        # If there is no commit for this line, print it without changes
        if commit is None:
            print(prefix)
            continue

        # If there is a commit, get the PR from it
        try:
            _, meta = parse_meta(commit.message)
            pr_num = meta['fel-pr']

            print("{}#{} {}".format(prefix, pr_num, commit.summary))

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
    land_parser.set_defaults(func=_land)

    status_parser = subparsers.add_parser('status')
    status_parser.set_defaults(func=_status)

    args = parser.parse_args()

    latest = requests.get('https://pypi.org/pypi/fel/json').json()['info']['version']
    if latest != __version__:
        print("You are running fel {}, the latest is {}".format(__version__, latest))

    if args.verbose:
        logging.basicConfig(level=logging.INFO)

    if args.version:
        print("fel {} from {}".format(__version__, __file__))
        return 0

    # Set default config values
    config = {
            'upstream': 'master',
    }

    # Read config file
    try:
        with open(args.config, "r") as config_yaml:
            loaded_config = yaml.safe_load(config_yaml)
            if loaded_config is not None:
                config.update(loaded_config)
    except IOError as ex:
        logging.error("Could not open config file: %s", ex)
        return 1

    # Check for required fields
    required_fields = ['gh_token']

    for field in required_fields:
        if field not in config:
            logging.error("Missing required config field: %s", field)
            return 2

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
    remote_url = next(repo.remote().urls)
    match = re.match(r"git@github.com:(.*/.*)\.git", remote_url)
    gh_slug = match.group(1)
    gh_repo = gh_client.get_repo(gh_slug)

    # Run the sub command
    args.func(repo, gh_repo, args, config)

    return 0

if __name__ == '__main__':
    main()

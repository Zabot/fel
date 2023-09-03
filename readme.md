# Fel
Fel submits each commit in a branch as a separate PR against a remote repo. Fel is designed
to be a companion to the conventional git tooling. Rather than needing to create and manage
stacks as first class objects, Fel infers the shape of a stack. You can continue to manipulate
your repo with `git` without needing to keep fel in sync as you make changes. Fel also does not
try and handle merging stacks, meaning it can be used without any support needed from the repo
the stack is submitted to.

## Usage
Create a new branch to represent your stack. The name of the branch
will be used as the name of the stack. Make commits to your branch,
each commit will be translated into a single PR. Amend commits as needed
with this in mind. Upload your branch to GitHub with `fel submit`. Fel will
automatically push a branch for every commit in your stack and create PRs with
properly configured base branches, so each commit appears as a single diff.

If you amend any of your commits, run `fel submit` again from the top of the stack.
Fel will force push the branches corresponding to each PR and post a message in each
thread with a diff between the newly submitted commit and the last commit.

Once your PRs are ready to merge, merge them as normal using the GitHub UI and rebase your
stack on top of the newly merged commit. Fel does not have an opinion on how stacks are
landed, only how they're created.

### Git config
Fel uses [git notes](https://git-scm.com/docs/git-notes) to track the metadata associated
with each commit. In order for fel to track commits across rebases and amends, you must
ensure that `notes.rewriteRef` includes `refs/notes/fel`. This config ensures that git
will persist the fel metadata when modifying commits with attached notes.
```ini
[notes]
	rewriteRef = "refs/notes/fel"
```

## Config
Fel reads from a config file in `~/.config/fel/config.toml`

```toml
token = "<github pat>" # The token used to create and modify PRs
default_remote = "origin" # The remote to push branches too and make PRs against
default_upstream = "master" # The branch of the remote to make PRs against
```

## TODO
- Properly check `XDG_CONFIG_DIRS` for config file
- Optionally make commit messages authoritative and overwrite pr body on every submit
- Status command to view PR status
- Include stack name and index in PR title

## What happened to the old version
Fel was previously a python script. It was getting untenable to maintain, and I've since learned rust. So I rewrote it in rust
The legacy python version is available [here](https://github.com/Zabot/fel/releases/tag/legacy-python)


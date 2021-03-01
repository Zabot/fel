# Fel
Fel is a tool for submitting [stacked diffs](https://medium.com/@kurtisnusbaum/stacked-diffs-keeping-phabricator-diffs-small-d9964f4dcfa6)
to GitHub. Fel takes care of all the busy work of submitting multiple commits as
a stack of PRs and lets you focus on keeping your diffs reviewable and lets reviewers
focus on understanding your code. When your stack is ready to land, Fel handles merging
all your PRs through GitHub, producing a commit history that looks like you rebased
the whole stack at once, without polluting your history with extra merge commits,
or requiring the upstream project to use an external tool to land diffs to master.

# Demo
![Fel Demo GIF](https://raw.githubusercontent.com/Zabot/fel/master/.images/demo.gif)

Fel even generates graphs for your PRs to indicate all of the diffs in your stack
and how they relate.

> This diff is part of a [fel stack](https://github.com/zabot/fel)
> <pre>
> * <a href="75">#75 Bugfixes in file 4</a>
> * <a href="74">#74 Added file4</a>
> | * <a href="73">#73 New line in third file</a>
> |/  
> * <a href="72">#72 Third new file</a>
> * <a href="71">#71 Line 1 in new file</a>
> * master
> </pre>


# Usage
Fel requires a GitHub oauth token to create and merge PRs on your behalf. Generate
one [here](https://github.com/settings/tokens). Once you have your token, add it
to the Fel configuration file (default `~/.fel.yml`).

```yaml
gh_token: <your_token_here>
```

Now create a new branch and start writing some diffs. Working with stacked diffs
requires a different way of thinking, think of each commit as an atomic unit of
change. Commit early into the development of each diff and amend often. Leave 
detailed commit bodies, they'll become the contents of your PRs when you submit
your stack for review.

Once your stack is ready, run `fel submit`. Fel will generate a PR for each commit
in the stack, basing the first PR against `origin/master`, and then each subsequent
PR against the previous PR in the stack. If multiple stacks overlap, Fel will
create a single PR for the common diffs, and base the diverging diffs on the common
base.

When your diffs are reviewed and ready to land, checkout the top of your stack
and run `fel land`. Fel will merge the PRs on GitHub in order by rebasing onto
the base branch, without creating the ladder of merge commits associated
with a manual stacked PR workflow. After your commits are landed, fel cleans up
the branches it generated and leaves you on a fresh checkout of the upstream branch,
with all of your diffs landed.


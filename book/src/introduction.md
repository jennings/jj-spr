# Introduction

**Super Pull Requests (SPR)** is the power tool for Jujutsu + GitHub workflows. It enables amend-friendly single PRs and effortless stacked PRs, bridging the gap between Jujutsu's change-based workflow and GitHub's pull request model.

> **⚠️ Important: Write Access Required**
>
> Due to GitHub API limitations, SPR requires **write access** to the repository. You must be a collaborator or have write permissions to use SPR. This is a GitHub platform constraint - the API does not support creating PRs from forks without write access to the target repository.
>
> If you're contributing to a project where you don't have write access, you'll need to use the standard fork + PR workflow through the GitHub web interface.

## Why SPR?

### For Everyone: Amend-Friendly PRs

- **Amend freely**: Use Jujutsu's natural `jj squash` and `jj describe` workflow
- **Review cleanly**: Reviewers see clear incremental diffs, not confusing force-push history
- **Update naturally**: Each update creates a new commit on GitHub, preserving review context
- **Land cleanly**: Everything squashes into one perfect commit on merge

### For Power Users: Effortless Stacking

- **Stack with confidence**: Create dependent or independent PRs with automatic rebase handling
- **Land flexibly**: Use `--cherry-pick` to land PRs in any order
- **Rebase trivially**: Jujutsu's stable change IDs survive rebases
- **Review independently**: Each PR shows only its changes, not the cumulative stack

## The Problem SPR Solves

Jujutsu encourages amending changes. GitHub's review UI breaks with force pushes. SPR bridges this gap by maintaining an append-only PR branch on GitHub while you amend freely locally.

When you update a PR with SPR:
1. Locally: You amend using Jujutsu's natural workflow
2. On GitHub: SPR creates a new commit showing your changes
3. For reviewers: They see clean "what changed since last review" diffs
4. At landing: Everything squashes into one commit on main

## What's Next?

- **New to SPR?** Start with [Installation](./user/installation.md) and [Setup](./user/setup.md)
- **Ready to create PRs?** Check out the [Single PR Workflow](./user/simple.md)
- **Want to stack PRs?** Learn about [Stacked PRs](./user/stack.md)
- **Need configuration help?** See the [Configuration Reference](./reference/configuration.md)

## Repository

SPR is open source and available at [github.com/LucioFranco/jj-spr](https://github.com/LucioFranco/jj-spr).

## Credits

Super Pull Requests builds on the foundation of:
- Original [spr](https://github.com/getcord/spr) by the Cord team
- [Jujutsu integration](https://github.com/sunshowers/spr) by sunshowers
- [Jujutsu](https://github.com/martinvonz/jj) by Martin von Zweigbergk and contributors

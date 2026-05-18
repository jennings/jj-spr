# Super Pull Requests (SPR)

**The power tool for Jujutsu + GitHub workflows. Single PRs with amend support. Stacked PRs without the complexity.**

A command-line tool that bridges Jujutsu's change-based workflow with GitHub's pull request model. Amend freely in your local repository while keeping reviewers happy with clean, incremental diffs.

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

**The Problem SPR Solves:**
Jujutsu encourages amending changes. GitHub's review UI breaks with force pushes. SPR bridges this gap by maintaining an append-only PR branch on GitHub while you amend freely locally.

## Quick Start

### Installation

#### From Source

```bash
git clone https://github.com/LucioFranco/jj-spr.git
cd jj-spr
cargo install --path spr
```

This installs the `jj-spr` binary to your `~/.cargo/bin` directory.

#### Set Up as Jujutsu Subcommand

Configure `jj spr` as a subcommand:

```bash
jj config set --user aliases.spr '["util", "exec", "--", "jj-spr"]'
```

<details>
<summary>Alternative configuration methods</summary>

**Manual Configuration:**
Add to your Jujutsu config (`~/.jjconfig.toml` or `.jj/repo/config.toml`):

```toml
[aliases]
spr = ["util", "exec", "--", "jj-spr"]
```

**Direct Binary Path:**
```toml
[aliases]
spr = ["util", "exec", "--", "/path/to/jj-spr"]
```
</details>

### Initial Setup

1. **Initialize in your repository:**
   ```bash
   cd your-jujutsu-repo
   jj spr init
   ```

2. **Provide your GitHub Personal Access Token** when prompted.

### Basic Workflow

The recommended workflow keeps an empty working copy (`@`) with PR changes at `@-`:

```bash
# 1. Create a change
jj new main@origin
echo "new feature" > feature.txt
jj describe -m "Add new feature"

# 2. Move to empty working copy
jj new  # Your PR change is now at @-

# 3. Submit for review
jj spr diff  # Creates PR for @-

# 4. Amend based on feedback
echo "updated feature" > feature.txt
jj squash  # Squash changes into @-
jj spr diff  # Updates PR with new commit (reviewers see clean diff)

# 5. Land when approved
jj spr land -r @-

# 6. Rebase after landing
jj git fetch
jj rebase -r @ -d main@origin
```

## Key Concepts

- **`@`** = your working copy (where you make edits)
- **`@-`** = parent of working copy (your PR change)
- **`jj spr diff`** defaults to `@-` (your completed change)
- **`jj spr land`** defaults to `@` (working copy)
- **Change IDs** remain stable through rebases, keeping PRs linked

## Commands

### Core Commands

- **`jj spr diff`** - Create or update a pull request
  - Updates create new commits on GitHub (reviewers see clean diffs)
  - Supports single changes or ranges: `-r @-`, `-r main..@`, `--all`
  - Cherry-pick mode: `--cherry-pick` for independent PRs

- **`jj spr land`** - Land (squash-merge) an approved pull request
  - Supports cherry-pick mode for landing PRs in any order
  - Requires manual rebase after landing (see docs)

- **`jj spr list`** - List open pull requests and their status

- **`jj spr close`** - Close a pull request

- **`jj spr amend`** - Update local commit message from GitHub

### Examples

```bash
# Single PR workflow
jj spr diff                    # Create/update PR for @-
jj spr land -r @-              # Land the PR

# Stacked PRs (dependent)
jj spr diff --all              # Create PRs for all changes
jj spr land -r <change-id>     # Land bottom of stack

# Independent PRs
jj spr diff --cherry-pick      # Create independent PR
jj spr land --cherry-pick -r <id>  # Land in any order

# Working with specific changes
jj spr diff -r <change-id>     # Update specific change
jj spr diff -r main..@         # Update range of changes
```

## Stacked Pull Requests

SPR excels at handling stacked PRs with two approaches:

### Independent Changes (Recommended)
Use `--cherry-pick` for changes that don't strictly depend on each other:
- Land in any order
- Simpler workflow
- Best for most use cases

### Dependent Stacks
For true dependencies where one change requires another:
- Automatic base branch handling
- Changes must land in order (parent → child)
- More complex but handles true dependencies

See the [stacking documentation](./docs/user/stack.md) for detailed workflows.

## Configuration

SPR stores configuration in your repository's git config:

```bash
# Set GitHub repository (if not auto-detected)
git config spr.githubRepository "owner/repo"

# Set branch prefix for generated branches
git config spr.branchPrefix "yourname/spr/"

# Require approval before landing
git config spr.requireApproval true
```

## Requirements

- **Repository Write Access**: You must have write permissions (collaborator status) on the target GitHub repository
- **Jujutsu**: Colocated Git repository (`jj git init --colocate`)
- **GitHub Access**: Personal Access Token with `repo` scope permissions
- **Git**: Git binary in PATH

## Documentation

Full documentation is available at **[luciofranco.github.io/jj-spr](https://luciofranco.github.io/jj-spr/)**

Quick links:
- [Installation](https://luciofranco.github.io/jj-spr/user/installation.html) - Detailed installation instructions
- [Setup](https://luciofranco.github.io/jj-spr/user/setup.html) - Initial configuration
- [Simple PR Workflow](https://luciofranco.github.io/jj-spr/user/simple.html) - Single PR workflow guide
- [Stacked PRs](https://luciofranco.github.io/jj-spr/user/stack.html) - Multi-PR workflows and stacking
- [Commit Messages](https://luciofranco.github.io/jj-spr/user/commit-message.html) - Message format and sections
- [Commands Reference](https://luciofranco.github.io/jj-spr/reference/commands.html) - Complete command reference
- [Configuration](https://luciofranco.github.io/jj-spr/reference/configuration.html) - All configuration options

## Contributing

Contributions welcome! Please:

1. Check existing issues before starting work
2. Add tests for new functionality
3. Follow existing code style (`cargo fmt` and `cargo clippy`)
4. Update documentation as needed

### Running Tests

```bash
# Run unit tests
cargo test

# Run integration tests (requires jj and git)
cargo test --test '*'

# Check code quality
cargo clippy --all-features --all-targets
cargo fmt --check
```

## Credits

Super Pull Requests builds on the foundation of:
- Original [spr](https://github.com/getcord/spr) by the Cord team
- [Jujutsu integration](https://github.com/sunshowers/spr) by sunshowers
- [Jujutsu](https://github.com/martinvonz/jj) by Martin von Zweigbergk and contributors

## License

MIT License - see [LICENSE](./LICENSE) for details.
